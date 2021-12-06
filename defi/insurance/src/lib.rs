use scrypto::prelude::*;


blueprint! {
    struct Insurance {
        /// Mint authorization to TL tokens.
        org_vault: Vault,
        org_badge: ResourceDef,

        // Non locked assets
        assets_pool: Vault,
        // Locked assets by policies
        locked_pool: Vault,
       
        // HashMap of policy address and details
        policies: HashMap<Address, Vault>,
        // HashMap of insurer and policy vaults
        purchases: HashMap<Address, Vault>,
    }

    impl Insurance {
        // Implement the functions and methods which will manage those resources and data
        
        // This is a function, and can be called directly on the blueprint once deployed
        pub fn new(base_assets: Bucket) -> (Component, Bucket) {
            scrypto_assert!(base_assets.amount() > Decimal::zero(), "Base assets cannot be zero");
            scrypto_assert!(base_assets.resource_def() == RADIX_TOKEN.into(), "You must use Radix (XRD).");

            let org_bucket = ResourceBuilder::new()
                .metadata("name", "Org Mint Auth")
                .new_badge_fixed(2);

            let org_resource_def = org_bucket.resource_def();
            let org_return_bucket: Bucket = org_bucket.take(1); // Return this badge to the caller

            let assets_def = base_assets.resource_def();
            let component = Self {
                org_vault: Vault::with_bucket(org_bucket),
                org_badge: org_resource_def,
                assets_pool: Vault::with_bucket(base_assets),
                locked_pool: Vault::new(assets_def),
                policies:HashMap::new(),
                purchases: HashMap::new()
            }
            .instantiate();
            (component,org_return_bucket)
        }

        
        // #[auth(org_badge)]
        pub fn make_policy(&mut self, policy_type: String, coverage: Decimal, price:Decimal, duration:u64, supply: Decimal){
            scrypto_assert!(coverage > Decimal::zero(), "Coverage cannot be zero");
            scrypto_assert!(price > Decimal::zero(), "Price cannot be zero");
            scrypto_assert!(duration > 0, "Duration cannot be zero");
            scrypto_assert!(supply > Decimal::zero(), "Supply cannot be zero");

            scrypto_assert!(self.assets_pool.amount() >= coverage * supply, "You don't have enough assets to cover this supply");

            // policy badge
            let ip_resource_def = ResourceBuilder::new()
                 .metadata("name", "Insurance Policy badge")
                 .metadata("symbol", policy_type)
                 .metadata("price", price.to_string())
                 .metadata("coverage", coverage.to_string())
                 .metadata("duration", duration.to_string())
                 .new_badge_mutable(self.org_vault.resource_def());

            info!("New policy address: {}", ip_resource_def.address());
            let bucket = self.org_vault.authorize(|badge| {
                ip_resource_def
                    .mint(supply, badge)
            });

            // new vault
            let vault = Vault::with_bucket(bucket);
            self.policies.insert(ip_resource_def.address(), vault);

            // lock assets
            let locked = self.assets_pool.take(coverage * supply);

            self.locked_pool.put(locked)
        }

        pub fn purchase(&mut self, policy_address: Address, insurer: Address, bucket: Bucket) -> Bucket {
            scrypto_assert!(self.policies.contains_key(&policy_address), "No policy found");
            scrypto_assert!(bucket.resource_def() == RADIX_TOKEN.into(), "You must purchase policies with Radix (XRD).");
            scrypto_assert!(!self.purchases.contains_key(&insurer), "This insurer already has purchases");


            let policies = self.policies.get(&policy_address).unwrap();
            scrypto_assert!(!policies.is_empty(), "No available policies");

            // take one policy
            let policy = policies.take(1);
            // get metadata
            let metadata = policy.resource_def().metadata();
            // check price
            let price:Decimal = metadata["price"].parse().unwrap();
            scrypto_assert!(bucket.amount() > price, "Not enough amount to purchase this policy");

            // check duration and setup the end epoch
            let duration:u64 = metadata["duration"].parse().unwrap();
            let expires = Context::current_epoch() + duration;

            // check coverage
            let coverage:Decimal = metadata["coverage"].parse().unwrap();

            let ip_resource_def = ResourceBuilder::new()
                .metadata("name", "Insurance Policy Token")
                .metadata("symbol", "IPT")
                .metadata("expires", expires.to_string())
                .metadata("policy", policy_address.to_string())
                .new_token_mutable(self.org_vault.resource_def());

            let ip_token = self.org_vault.authorize(|badge| {
                ip_resource_def
                    .mint(coverage, badge)
            });
            
            let vault = Vault::with_bucket(ip_token);

            // burn taken policy
            self.org_vault.authorize(|badge| {
                policy.burn(badge);
            });

            // store the vault
            self.purchases.insert(insurer, vault);

            // take payment
            let payment = bucket.take(price);
            self.assets_pool.put(payment);

            // return the rest bucket
            bucket
        }

        //#[auth(org_badge)]
        pub fn approve(&mut self, insurer: Address, amount: Decimal){
            scrypto_assert!(self.purchases.contains_key(&insurer), "No vault found for this insurer");

            let purchase = self.purchases.get(&insurer).unwrap();
            let bucket = purchase.take(amount);

            // get metadata
            let metadata = bucket.resource_def().metadata();
            let expires:u64 = metadata["expires"].parse().unwrap();
            scrypto_assert!(Context::current_epoch() < expires, "Policy is expired");

            let supply = bucket.amount();
            // Burn the TL tokens received
            self.org_vault.authorize(|badge| {
                bucket.burn(badge);
            });
        

            Account::from(insurer).deposit(self.locked_pool.take(supply));   
        }

        //#[auth(org_badge)]
        pub fn burn_purchases(&mut self, insurer: Address) {
            scrypto_assert!(self.purchases.contains_key(&insurer), "No vault found for this insurer");

            let purchase = self.purchases.get(&insurer).unwrap();
            let bucket = purchase.take_all();

            let metadata = bucket.resource_def().metadata();
            let expires:u64 = metadata["expires"].parse().unwrap();
            scrypto_assert!(Context::current_epoch() > expires, "Policy is not expired");

            let supply = bucket.amount();
            // Burn the the rest purchases
            self.org_vault.authorize(|badge| {
                bucket.burn(badge);
            });

            // unlock assets
            self.assets_pool.put(self.locked_pool.take(supply));

            // clear purchases
            self.purchases.remove(&insurer);
        }

        //#[auth(org_badge)]
        pub fn burn_policies(&mut self, policy_address: Address) {
            scrypto_assert!(self.policies.contains_key(&policy_address), "No policy found");

            let policies = self.policies.get(&policy_address).unwrap();
            let bucket = policies.take_all();

            let metadata = bucket.resource_def().metadata();
            let coverage:Decimal = metadata["coverage"].parse().unwrap();
           
            let supply = bucket.amount()*coverage;
            // Burn the the rest purchases
            self.org_vault.authorize(|badge| {
                bucket.burn(badge);
            });

            // unlock assets
            self.assets_pool.put(self.locked_pool.take(supply));

            // clear policies
            self.policies.remove(&policy_address);
        }

        /// Org assets methods
        // #[auth(org_badge)]
        pub fn deposit(&mut self, bucket: Bucket) {
            scrypto_assert!(bucket.amount() > Decimal::zero(), "You cannot deposit zero amount");

            self.assets_pool.put(bucket)
        }

        // #[auth(org_badge)]
        pub fn withdraw(&mut self, amount: Decimal) -> Bucket {
            scrypto_assert!(self.assets_pool.amount() >= amount, "Withdraw amount is bigger than available assets");

            self.assets_pool.take(amount)
        }

        // #[auth(org_badge)]
        pub fn assets(&mut self) -> Decimal {
            self.assets_pool.amount()
        }

        // #[auth(org_badge)]
        pub fn locked(&mut self) -> Decimal {
            self.locked_pool.amount()
        }
    }
}
