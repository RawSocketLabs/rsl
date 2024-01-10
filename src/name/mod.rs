/*!
A NetBIOS Name Service (NBNS) packet header.

The header is constructed via the HeaderBuilder. The builder exposes every
field in the header.


# Example

A simple example of constructing a header and writing it to a vector is as follows:

```
use nbt::{RequestBuilder, RequestOp};

fn main() {
    // Create a new request and get its transactino id.
    let (transaction_id, request) = RequestBuilder::default()
        .name("testname.test.local".into())
        .op(RequestOp::Register)
        .build()
        .unwrap()
        .generate()
        .unwrap();

    // Print out the request bytes.
    println!("{:?}", request.as_bytes());
}
```
*/

mod client;
mod error;
mod flags;
mod header;
mod label;
mod opcode;
mod question;
mod rcode;
mod request;
mod resource;
mod response;
mod state;

pub use client::*;
pub use error::*;
pub use flags::*;
pub use header::*;
pub use label::*;
pub use opcode::*;
pub use question::*;
pub use rcode::*;
pub use request::*;
pub use resource::*;
pub use response::*;
use state::*;
