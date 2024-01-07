/*!
A NetBIOS Name Service (NBNS) packet header.

The header is constructed via the HeaderBuilder. The builder exposes every
field in the header.


# Example

A simple example of constructing a header and writing it to a vector is as follows:

```
use binrw::{BinRead, BinWrite, io::Cursor};
use nbt::name::{HeaderBuilder, Op, OpCode, Flags, RCode, Query};

fn main() {
    let header = HeaderBuilder::default()
       .transacition_id(0x1234)
       .opcode(OpCode::new().with_op(Op::Query).with_response(false))
       .flags(Flags::new())
       .rcode(Query::Success.into())
       .check_soundness(false)
       .build()
       .unwrap();

    let mut buffer = Cursor::new(Vec::new());
    header.write(&mut buffer).unwrap();
    println!("{:?}", buffer.into_inner());
}
```
*/

mod error;
mod flags;
mod header;
mod label;
mod opcode;
mod question;
mod rcode;
mod resource;
mod state;

pub use error::*;
pub use flags::*;
pub use header::*;
pub use label::*;
pub use opcode::*;
pub use question::*;
pub use rcode::*;
pub use resource::*;
use state::*;
