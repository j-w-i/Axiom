use std::collections::HashMap;

fn main() {
    let mut counts: HashMap<&str, i32> = HashMap::new();
    counts.insert("a", 1);
    println!("{counts:?}");
}
