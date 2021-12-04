use sbor::*;
use scrypto::prelude::*;

#[derive(TypeId, Encode, Decode)]
struct Policy {
    duration:u64,
    amount: Decimal,
    price: Decimal
}

#[derive(TypeId, Encode, Decode)]
struct PurchasedPolicy {
    epoch: u64,
    insurer: Address,
    claimed: Decimal,
    applied: Decimal
}


blueprint! {
    struct Insurance {
        // Non locked assets
        assets_pool: Vault,
        // Locked assets by policies
        locked_pool: Vault,
        // 
        company_badge: ResourceDef,
        // 
        policies: HashMap<String, Policy>,
        purchased_policies: HashMap<String, PurchasedPolicy>,
    }

    impl Insurance {
        // Implement the functions and methods which will manage those resources and data
        
        // This is a function, and can be called directly on the blueprint once deployed
        pub fn new(base_assets: Bucket) -> (Component, Bucket) {
            scrypto_assert!(base_assets.amount() > Decimal::zero(), "Base assets cannot be zero");

            let company_badge_bucket = ResourceBuilder::new()
                 .metadata("name", "Company Badge")
                 .metadata("symbol", "CB")
                 .new_badge_fixed(1);


            let assets_def = base_assets.resource_def();
            let component = Self {
                assets_pool: Vault::with_bucket(base_assets),
                locked_pool: Vault::new(assets_def),
                company_badge:company_badge_bucket.resource_def(),
                policies:HashMap::new(),
                purchased_policies: HashMap::new()
            }
            .instantiate();
            (component,company_badge_bucket)
        }

        // #[auth(company_badge)]
        pub fn deposit(&mut self, bucket: Bucket) {
            scrypto_assert!(bucket.amount() > Decimal::zero(), "You cannot deposit zero amount");

            self.assets_pool.put(bucket)
        }

        // #[auth(company_badge)]
        pub fn withdraw(&mut self, amount: Decimal) -> Bucket {
            scrypto_assert!(self.assets_pool.amount() >= amount, "Withdraw amount is bigger than available assets");

            self.assets_pool.take(amount)
        }

        // #[auth(company_badge)]
        pub fn assets(&mut self) -> Decimal {
            self.assets_pool.amount()
        }

        // #[auth(company_badge)]
        pub fn locked(&mut self) -> Decimal {
            self.locked_pool.amount()
        }

        // #[auth(company_badge)]
        pub fn new_policy(&mut self, uuid: String, amount: Decimal, price:Decimal, duration:u64){
            scrypto_assert!(self.assets_pool.amount() > amount, "You don't have enough assets to cover this policy");
            

            // new policy
            let policy = Policy { duration, amount, price };
            self.policies.insert(uuid, policy);

            // lock assets
            let locked = self.assets_pool.take(amount);

            self.locked_pool.put(locked)
        }

        pub fn purchase(&mut self, uuid: String, bucket: Bucket) -> Bucket {
            scrypto_assert!(self.policies.contains_key(&uuid), "No policy found");
            scrypto_assert!(!self.purchased_policies.contains_key(&uuid), "This policy is already purchased");

            let policy = self.policies.get(&uuid).unwrap();
            scrypto_assert!(bucket.amount() > policy.amount, "Not enough amount to purchase this policy");

            let purchased_policy = PurchasedPolicy { 
                epoch: Context::current_epoch() + policy.duration, 
                insurer: bucket.resource_address(), 
                claimed: Decimal::zero(),
                applied: Decimal::zero()
            };

            self.purchased_policies.insert(uuid, purchased_policy);

            // take payment
            let payment = bucket.take(policy.price);
            self.assets_pool.put(payment);

            bucket
        }

        pub fn claim(&mut self, uuid: String, insurer: Address, amount: Decimal) {
            scrypto_assert!(self.purchased_policies.contains_key(&uuid), "This policy is not purchased");
            let policy = self.policies.get(&uuid).unwrap();

            let purchased = self.purchased_policies.get_mut(&uuid).unwrap();
            scrypto_assert!(purchased.insurer == insurer, "This policy doesn't belong to this insurer");
            // we check the epoch only for claims. Once the claim was made, we allow approve and apply even if policy is expired
            scrypto_assert!(Context::current_epoch() <= purchased.epoch, "This policy is expired");
            scrypto_assert!(purchased.claimed == Decimal::zero(), "The policy already has claimed amount");
            scrypto_assert!(amount <= policy.amount-purchased.applied, "Claimed amount is bigger than the policy can allow");

            (*purchased).claimed = amount;
            // let updated = PurchasedPolicy {
            //     epoch: purchased.epoch, 
            //     insurer: purchased.insurer, 
            //     claimed: amount,
            //     approved: purchased.approved,
            //     applied: purchased.applied
            // };

            // self.purchased_policies.insert(uuid, updated);
        }

        // #[auth(company_badge)]
        pub fn approve(&mut self, uuid: String, amount: Decimal) {
            scrypto_assert!(self.purchased_policies.contains_key(&uuid), "This policy is not purchased");

            let purchased = self.purchased_policies.get_mut(&uuid).unwrap();
            scrypto_assert!(amount <= purchased.claimed, "Approved amount cannot be bigger than claimed");

            // let updated = PurchasedPolicy {
            //     epoch: purchased.epoch, 
            //     insurer: purchased.insurer, 
            //     claimed: Decimal::zero(), //we reset claimed amount even though it can be bigger than the approved
            //     approved: amount,
            //     applied: purchased.applied
            // };


            // self.purchased_policies.insert(uuid, updated);
            (*purchased).claimed = Decimal::zero();
            (*purchased).applied += amount;

            let insurer = purchased.insurer;
            let payment = self.locked_pool.take(amount);
            let vault = Vault::new(insurer);
            vault.put(payment);
        }

        // #[auth(company_badge)]
        pub fn expire(&mut self, uuid: String) {
            scrypto_assert!(self.purchased_policies.contains_key(&uuid), "This policy is not purchased");

            let purchased = self.purchased_policies.get(&uuid).unwrap();
            scrypto_assert!(Context::current_epoch() > purchased.epoch, "Purchased policy is not expired yet");

            let policy = self.policies.get(&uuid).unwrap();

            let locked = self.locked_pool.take(policy.amount - purchased.applied);

            self.assets_pool.put(locked);

            self.purchased_policies.remove_entry(&uuid);
        }

        // #[auth(company_badge)]
        pub fn remove(&mut self, uuid: String) {
            scrypto_assert!(!self.purchased_policies.contains_key(&uuid), "This policy is purchased");

            let policy = self.policies.get(&uuid).unwrap();

            let locked = self.locked_pool.take(policy.amount);

            self.assets_pool.put(locked);

            self.policies.remove_entry(&uuid);
        }

        // TODO: add this as a feature
        // pub fn purchased(&mut self, address: Address) {
        //     // return a list of purchases
        // }


        // TODO: rethink about the policies as possible token instead. check the candy store
        // pros: using token allows to replace purchased certificate so it will contain the epoch. Insurer can sell the cert as they want
        // There could be a burn function for the entire token. Policy can hold the Vault, so no need to hold locked amount. After burning the policy can be removed
        // cons: need to find out how to mark some policy as purchased. This solution will not allow to claim and approve multiple times
        // in purpose to hold the vault the policy should be in the blueprint

        // or

        // we can actually replace Policy with a token and metadata that we can use for claim and approve. But it can only be as one time payment
        // by claiming, the token can be added into a burning hash map
        // once the policy with some amount is approved, the token can be burned
    }
}
