pub mod sample {
    include!(concat!(env!("OUT_DIR"), "/sample.rs"));
}

use sample::User;
use prost::Message;

fn main() {
    let user = User {
        id: 42,
        name: "Dhivijit".to_string(),
        email: "dhivijit@example.com".to_string(),
    };

    // Serialize
    let mut buf = Vec::new();
    user.encode(&mut buf).unwrap();
    println!("Serialized: {:?}", buf);

    // Deserialize
    let decoded = User::decode(&*buf).unwrap();
    println!("Decoded user = {:?}", decoded);
}
