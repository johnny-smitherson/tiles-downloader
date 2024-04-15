use catalytic_table_to_struct::transformer::DefaultTransformer;
use std::env::current_dir;

fn main() {
    dotenvy::dotenv().unwrap();
    let x = &current_dir().unwrap().join("src").join("generated");
    println!("PATH {:?}", x);
    // 1
    catalytic_table_to_struct::generate(
        // 2
        x,
        // 3
        DefaultTransformer,
    );
}
