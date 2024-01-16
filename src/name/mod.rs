/*!
A NetBIOS Name Service (NBNS) packet header.

The header is constructed via the HeaderBuilder. The builder exposes every
field in the header.


# Example

A simple example of constructing a header and writing it to a vector is as follows:

```
use nbt::name::{RequestBuilder, RequestOp};

fn main() {
    // Create a new request and get its transactino id.
    let (transaction_id, request) = RequestBuilder::default()
        .name("testname.test.local".into())
        .op(RequestOp::Register)
        .ttl(None)
        .flags(None)
        .address(None)
        .build()
        .unwrap()
        .generate()
        .unwrap();

    // Print out the request bytes.
    println!("{:?}", request.as_bytes());
}
```
*/

mod error;
pub mod header;

pub mod label;
mod name;
pub mod names {
    pub use super::name::*;
}
pub mod question;
mod request;
pub mod resource;
mod response;
mod state;

mod opcode;
mod rcode;
pub mod codes {
    pub use super::opcode::*;
    pub use super::rcode::*;
}

pub use error::*;
pub use request::*;
pub use response::*;
use state::*;
