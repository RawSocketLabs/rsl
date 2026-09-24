/*!
A NetBIOS Name Service (NBNS) packet header.

The header is constructed via the HeaderBuilder. The builder exposes every
field in the header.


# Example

A simple example of constructing a header and writing it to a vector is as follows:

```
use nbt::name::{RequestBuilder, RequestOp};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new request and get its transactino id.
    let (transaction_id, request) = RequestBuilder::default()
        .name("testname.test.local".into())
        .op(RequestOp::Register)
        .build()?
        .generate()?;

    // Print out the request bytes.
    println!("{:?}", request.as_bytes());
    Ok(())
}
```
*/

mod error;

/// The NetBIOS Name Service (NBNS) packet header.
pub mod header;

/// Name, Pointer, and Custom labels for NBNS entries.
pub mod label;

mod name;
/// NetBIOS Name utiltiies.
pub mod names {
    pub use super::name::*;
}

/// Question structure for NBNS requests.
pub mod question;
mod request;

/// Resource structure for NBNS responses.
pub mod resource;
mod response;
mod state;

mod opcode;
mod rcode;

/// Response and Operation Codes for NBNS.
pub mod codes {
    pub use super::opcode::*;
    pub use super::rcode::*;
}

pub use error::*;
pub use request::*;
pub use response::*;
use state::*;
