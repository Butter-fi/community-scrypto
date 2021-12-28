use scrypto::prelude::*;

#[derive(NftData)]
pub struct AcademyCourse {
    skill: String,
    instructor: Address,
    #[scrypto(mutable)]
    resource_url: String,
    #[scrypto(mutable)]
    price: Decimal,
    #[scrypto(mutable)]
    students: u128,
    #[scrypto(mutable)]
    approved: bool
}


blueprint! {
    struct Academy {
        admin_badge: ResourceDef,
        courses_minter: Vault,
        fee: Decimal,
        collected_fees: Vault,
        courses: Vault,
        courses_id_counter: u128,
        students: HashMap<Address, Vec<u128>>,
        instructors_vaults: HashMap<Address, Vault>
    }

    impl Academy {
        // 15% -> Academy, 85% instructor 
        pub fn new(academy_fee: Decimal) -> (Component, Bucket) {

            // admin is able to approve course
            let admin_badge = ResourceBuilder::new_fungible(DIVISIBILITY_NONE)
             .metadata("name", "Academy Admin Badge")
             .initial_supply_fungible(1);
            let admin_resource_def = admin_badge.resource_def();

            let mint_badge = ResourceBuilder::new_fungible(DIVISIBILITY_NONE)
                .metadata("name", "Academy Course Mint Badge")
                .initial_supply_fungible(1);

            // NFT course 
            let course_resource_def = ResourceBuilder::new_non_fungible()
            .metadata("name", "Academy course")
            .flags(MINTABLE | INDIVIDUAL_METADATA_MUTABLE)
            .badge(
                mint_badge.resource_def(),
                MAY_MINT | MAY_CHANGE_INDIVIDUAL_METADATA,
            )
            .no_initial_supply();

            let component = Self {
                admin_badge: admin_resource_def, 
                courses_minter: Vault::with_bucket(mint_badge), 
                fee: academy_fee,
                collected_fees: Vault::new(RADIX_TOKEN),
                courses: Vault::new(course_resource_def),
                courses_id_counter: 0,
                students: HashMap::new(),
                instructors_vaults: HashMap::new()
            }
            .instantiate();

            (component, admin_badge)
        }

        // register a user. Users can become instructors also
        // only instructors are allowed to register course and get rewards
        // only students allow to buy courses and rate them
        pub fn new_user(&self) -> Bucket {
            ResourceBuilder::new_fungible(DIVISIBILITY_NONE)
                .metadata("name", "Academy User Badge")
                .initial_supply_fungible(1)
        }

        pub fn register_course(&mut self, user_auth: BucketRef, price: Decimal, skill: String, resource_url: String) {
            let user_id = Self::get_user_id(user_auth);

            // mint course NFT
            let new_course = AcademyCourse {
                skill,
                price,
                resource_url,
                instructor: user_id,
                students: 0,
                approved: false
            };

            let bucket = self.courses_minter.authorize(|auth| {
                self.courses.resource_def()
                    .mint_nft(self.courses_id_counter, new_course, auth)
            });
            self.courses_id_counter += 1;

            // save the course
            self.courses.put(bucket);
        }
           

        #[auth(admin_badge)]
        pub fn approve_course(&mut self, id: u128) {
            // find course
            let mut course: AcademyCourse = self.courses.get_nft_data(id);
            // set this course as approved
            course.approved = true;

            self.courses_minter
                .authorize(|auth| self.courses.update_nft_data(id, course, auth));
        }

        pub fn buy_course(&mut self, user_auth: BucketRef, id: u128, payment: Bucket) -> Bucket {
            assert!(payment.resource_address() == RADIX_TOKEN, "You can only use radix (RDX)");

            // get user
            let user_id = Self::get_user_id(user_auth);

            // make sure we are not a student of this course already
            let student_courses = self.students.entry(user_id).or_insert(Vec::new());
            assert!(!student_courses.contains(&id), "This course is already purchased");

            // get course NFT
            let mut course: AcademyCourse = self.courses.get_nft_data(id);
            // check that the course is approved
            assert!(course.approved, "This course is not approved yet");
            // check that we are not instructor
            assert!(course.instructor != user_id, "Instructors are not allowed to buy own courses");
            // check price
            assert!(course.price <= payment.amount(), "Not enough XRD to buy this course");

            // calculate rewards
            let bucket = payment.take(course.price);
            let academy_reward = bucket.amount() * self.fee/100;
            // send portion to academy wallet
            self.collected_fees.put(bucket.take(academy_reward));
            // send the rest to instructor's vault
            let vault = self.instructors_vaults.entry(course.instructor).or_insert(Vault::new(RADIX_TOKEN));
            vault.put(bucket);

            // update students count for NFT
            course.students += 1;

            self.courses_minter
                .authorize(|auth| self.courses.update_nft_data(id, course, auth));

            // save purchased course
            student_courses.push(id);

            // return rest amount
            payment
        }

        pub fn my_courses(&mut self, user_auth: BucketRef){
            let user_id = Self::get_user_id(user_auth);

            assert!(self.students.contains_key(&user_id), "This student has no courses purchased");
            let courses = self.students.get(&user_id).unwrap();

            for id in courses {
                let course: AcademyCourse = self.courses.get_nft_data(*id);

                info!("Course: #{} {} {}", id, course.skill, course.resource_url);
            }
        }

        pub fn courses(&mut self) {
            
            let courses = self.courses.get_nfts::<AcademyCourse>();

            for course in courses {

                let data = course.data();

                if data.approved {
                    info!("Course: #{} {} {}", course.id(), data.skill, data.resource_url);
                }
            }
        }

        pub fn withdraw_reward(&mut self, user_auth: BucketRef) -> Bucket {
            let user_id = Self::get_user_id(user_auth);

            // only for instructors
            assert!(self.instructors_vaults.contains_key(&user_id), "No vault found for this instructor");

            let vault = self.instructors_vaults.get(&user_id).unwrap();
            vault.take_all()
        }

        #[auth(admin_badge)]
        pub fn withdraw_fee(&mut self) -> Bucket {
            self.collected_fees.take_all()
        }

        fn get_user_id(user_auth: BucketRef) -> Address {
            assert!(user_auth.amount() > 0.into(), "Invalid user proof");
            let user_id = user_auth.resource_address();
            user_auth.drop();
            user_id
        }
    }
}
