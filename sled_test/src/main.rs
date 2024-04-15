use serde::{Deserialize, Serialize};
use std::env::current_dir;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SomeValue(u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Person {
    pub name: String,
    pub age: i32,
    pub email: String,
    pub json_example: String,
}
       // serde types are also supported!!!
       #[derive(Debug, Serialize, Deserialize)]
       struct Hello<'a> {
           string: &'a str,
       }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // main_sled()
    main_heed()
}

fn main_heed() -> Result<(), Box<dyn std::error::Error>>  {
    use heed::types::*;
    use heed::{Database, EnvOpenOptions};
    let path = std::path::Path::new("data.heed").join("heed.mdb");

    std::fs::create_dir_all(&path)?;

    let env = unsafe {
        heed::EnvOpenOptions::new()
            .map_size(1000 * 1024 * 1024) // 10MB
            .max_dbs(3000)
            .open(path)?
    };

 

    let db: heed::Database<Str, SerdeBincode<Hello>> =
        env.create_database(Some("serde-bincode"))?;

    let mut tx = env.write_txn()?;

    let hello = Hello { string: "hi" };
    db.put(&mut tx, "hello", &hello)?;


    let ret: Option<Hello> = db.get(&mut tx, "hello")?;
    println!("serde-bincode:\t{:?}", ret);

    tx.commit()?;

    

    for _ in 0..1000 {
        benchmark_heed(&env)?;
    }

    Ok(())
}

fn benchmark_heed(env: &heed::Env)  -> Result<(), Box<dyn std::error::Error>> {
    use heed::types::*;
    let db2: heed::Database<Str, SerdeBincode<Person>> = env.create_database(Some("person"))?;
    let mut tx = env.write_txn()?;

    let mut rng = rand::thread_rng();
    use rand::Rng;

    use std::time::Instant;
    let t0 = Instant::now();

    const  item_count: i32 = 1000;

    for i in 0..item_count {
        let val = rng.gen::<i32>();
        let name = format!("test_{}", val);
        let age = val % 10;
        let email = format!("{}@test.com", val);
        let person = Person {
            name: name.clone(),
            age,
            email,
            json_example: "not important".to_string(),
        };
        db2.put(&mut tx, name.as_str(), &person)?;
    }

    tx.commit()?;

    let dt = t0.elapsed();
    println!("Inserted: item_count in {:.2?} ms", dt.as_millis());

    Ok(())
}

fn main_sled() -> Result<(), Box<dyn std::error::Error>> {
    // Creating a temporary sled database.
    // If you want to persist the data use sled::open instead.
    //let db = sled::Config::new().temporary(true).open().unwrap();
    let x = current_dir().unwrap().join("data.sled");
    let db = sled::open(x)?;

    // The id is used by sled to identify which Tree in the database (db) to open
    let tree = typed_sled::Tree::<String, SomeValue>::open(&db, "unique_id");

    tree.insert(&"some_key".to_owned(), &SomeValue(10))?;

    assert_eq!(tree.get(&"some_key".to_owned())?, Some(SomeValue(10)));

    let tree2 = typed_sled::Tree::<String, Person>::open(&db, "benchmark");
    for _ in 0..1000 {
        benchmark_insert(&tree2);

    }
    Ok(())
}

 fn benchmark_insert(db: &typed_sled::Tree::<String, Person>) {
    let mut rng = rand::thread_rng();
    use rand::Rng;

    use std::time::Instant;
    let t0 = Instant::now();

    const  item_count: i32 = 1000;

    for i in 0..item_count {
        let val = rng.gen::<i32>();
        let name = format!("test_{}", val);
        let age = val % 10;
        let email = format!("{}@test.com", val);
        let person = Person {
            name,
            age,
            email,
            json_example: "not important".to_string(),
        };
        db.insert(&person.name.to_owned(), &person).expect("failed to insert");
    }

    let dt = t0.elapsed();
    println!("Inserted: item_count in {:.2?} ms", dt.as_millis());
}