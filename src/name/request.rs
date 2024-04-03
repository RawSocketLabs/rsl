use derive_builder::Builder;
use rand::Rng;

use crate::name::codes::{Op, OpCode, QueryCode};
use crate::name::header::{Flags, Header, HeaderBuilder};
use crate::name::label::{NameLabel, PointerLabel};
use crate::name::question::{QuestionBuilder, QuestionClass, QuestionType};
use crate::name::resource::{
    Record, RecordAddress, RecordFlags, ResourceBuilder, ResourceClass, ResourceType,
};

use super::RequestGenerationError;

/// The set of defined request operations supported in the RFC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestOp {
    /// Overwrite the existing name.
    ///
    /// # Key Features:
    /// - **OpCode:** [Registration](crate::name::codes::Op::Registration)
    /// - **Flags:** `None` (Optional: [Broadcast](crate::name::header::Flags::broadcast))
    ///
    /// # Diagram:
    /// The generated packet will have the following form:
    /// ```text
    ///                       1 1 1 1 1 1 1 1 1 1 2 2 2 2 2 2 2 2 2 2 3 3
    ///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |          NAME_TRN_ID          |0|  0x5  |0|0|0|0|0 0|B|  0x0  |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0001             |            0x0000             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0000             |            0x0001             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                         QUESTION_NAME                         /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                            RR_NAME                            /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                              TTL                              |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |              0x0006           |            NB_FLAGS           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                          NB_ADDRESS                           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    /// ```
    Overwrite,

    /// Query for the given name.
    ///
    /// # Key Features:
    /// - **OpCode:** [Query](crate::name::codes::Op::Query)
    /// - **Flags:** [Recursion Desired](crate::name::header::Flags::recursion_desired)
    ///
    /// # Diagram:
    /// The generated packet will have the following form:
    /// ```text
    ///                       1 1 1 1 1 1 1 1 1 1 2 2 2 2 2 2 2 2 2 2 3 3
    ///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |          NAME_TRN_ID          |0|  0x0  |0|0|1|0|0 0|B|  0x0  |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0001             |            0x0000             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0000             |            0x0000             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                         QUESTION_NAME                         /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    /// ```
    Query,

    /// Refresh the current name.
    ///
    /// # Key Features:
    /// - **OpCode:** [Refresh](crate::name::codes::Op::Refresh)
    /// - **Flags:** `None` (Optional: [Broadcast](crate::name::header::Flags::broadcast))])
    ///
    /// # Diagram:
    /// The generated packet will have the following form:
    /// ```text
    ///                       1 1 1 1 1 1 1 1 1 1 2 2 2 2 2 2 2 2 2 2 3 3
    ///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |          NAME_TRN_ID          |0|  0x9  |0|0|0|0|0 0|B|  0x0  |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0001             |            0x0000             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0000             |            0x0001             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                         QUESTION_NAME                         /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                            RR_NAME                            /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                              TTL                              |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |              0x0006           |            NB_FLAGS           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                          NB_ADDRESS                           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    /// ```
    Refresh,

    /// Register a new name.
    ///
    /// # Key Features:
    /// - **OpCode:** [Registration](crate::name::codes::Op::Registration)
    /// - **Flags:** [Recursion Desired](crate::name::header::Flags::recursion_desired)
    ///
    /// # Diagram:
    /// The generated packet will have the following form:
    /// ```text
    ///                       1 1 1 1 1 1 1 1 1 1 2 2 2 2 2 2 2 2 2 2 3 3
    ///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |          NAME_TRN_ID          |0|  0x5  |0|0|0|0|0 0|B|  0x0  |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0001             |            0x0000             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0000             |            0x0001             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                         QUESTION_NAME                         /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                            RR_NAME                            /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                              TTL                              |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |              0x0006           |            NB_FLAGS           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                          NB_ADDRESS                           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    /// ```
    Register,

    /// Release the current name.
    ///
    /// # Key Features:
    /// - **OpCode:** [Release](crate::name::codes::Op::Release)
    /// - **Flags:** `None` (Optional: [Broadcast](crate::name::header::Flags::broadcast))])
    ///
    /// # Diagram:
    /// The generated packet will have the following form:
    /// ```text
    ///                       1 1 1 1 1 1 1 1 1 1 2 2 2 2 2 2 2 2 2 2 3 3
    ///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |          NAME_TRN_ID          |0|  0x6  |0|0|0|0|0 0|B|  0x0  |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0001             |            0x0000             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0000             |            0x0001             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                         QUESTION_NAME                         /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                            RR_NAME                            /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            NB (0x0020)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                              TTL                              |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |              0x0006           |            NB_FLAGS           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                          NB_ADDRESS                           |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    /// ```
    Release,

    /// Get the status of the current named node.
    ///
    /// # Key Features:
    /// - **OpCode:** [Query](crate::name::codes::Op::Query)
    /// - **Flags:** `None` (Optional: [Broadcast](crate::name::header::Flags::broadcast))
    /// - **Question Type:** [NBSTAT](crate::name::question::QuestionType::NBSTAT)
    ///
    /// # Diagram:
    /// The generated packet will have the following form:
    /// ```text
    ///                       1 1 1 1 1 1 1 1 1 1 2 2 2 2 2 2 2 2 2 2 3 3
    ///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |          NAME_TRN_ID          |0|  0x0  |0|0|0|0|0 0|B|  0x0  |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0001             |            0x0000             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |            0x0000             |            0x0001             |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |                                                               |
    ///  /                         QUESTION_NAME                         /
    ///  /                                                               /
    ///  |                                                               |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///  |        NBSTAT (0x0021)        |          IN (0x0001)          |
    ///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    /// ```
    Status,
}

/// The request information required to gererate a request header for a request defined in the RFC.
#[derive(Builder, Debug, Clone)]
pub struct Request {
    /// The name to preform the requested oepration against.
    pub name: String,

    /// The request operation as defined in the RFC to preform with this request.
    pub op: RequestOp,

    /// The time to live for the requested operation.
    #[builder(default)]
    pub ttl: Option<u32>,

    /// The flags for the requested operation.
    #[builder(default)]
    pub flags: Option<RecordFlags>,

    /// The address for the requested operation.
    #[builder(default)]
    pub address: Option<RecordAddress>,
}

impl Request {
    /// Generate a transaction id and header for an request operation as defined in the RFC.
    pub fn generate(self) -> Result<(u16, Header), RequestGenerationError> {
        // Randomly generate a unique transaction id.
        let mut rng = rand::thread_rng();
        let id = rng.gen_range(0..=65_535);

        // Attempt to create the name label
        let name = NameLabel::new(self.name).map_err(|_| RequestGenerationError::NameError)?;

        // Set the question to the name we want to register.
        let question_type = match self.op {
            RequestOp::Status => QuestionType::NBSTAT,
            _ => QuestionType::NB,
        };
        let question = QuestionBuilder::default()
            .name(vec![name.into()])
            .qtype(question_type)
            .class(QuestionClass::Internet)
            .build()
            .unwrap();

        // Use the provided values or use the default values
        let ttl = self.ttl.unwrap_or(3600);
        let flags = self.flags.unwrap_or(RecordFlags::new().with_group(false));
        let address = self
            .address
            .unwrap_or(RecordAddress::new("192.168.1.2".parse().unwrap()));

        // Create a record
        let record = Record::new(flags, address);

        // Create the resource
        let resource = ResourceBuilder::default()
            // TODO: Fix the offset this will take some calculation based on input information.
            .name(vec![PointerLabel::from_offset(0).unwrap().into()])
            .rtype(ResourceType::NB)
            .rclass(ResourceClass::Internet)
            .ttl(ttl)
            .length(6)
            // TODO: This probably makes sense to be a into via builder
            .data(record.into())
            .build()
            .unwrap();

        // Set the flags depending on the type of operation being set.
        let mut hflags = Flags::new();
        let mut opcode = OpCode::new();

        // Set the opcode and flags based on the requested operation.
        match self.op {
            RequestOp::Register => {
                hflags.set_recursion_desired(true);

                opcode.set_response(false);
                opcode.set_op(Op::Registration);
            }
            RequestOp::Overwrite => {
                opcode.set_response(false);
                opcode.set_op(Op::Registration);
            }
            RequestOp::Refresh => {
                opcode.set_response(false);
                opcode.set_op(Op::Registration);
            }
            RequestOp::Release => {
                opcode.set_response(false);
                opcode.set_op(Op::Release);
            }
            RequestOp::Query => {
                hflags.set_recursion_desired(true);

                opcode.set_response(false);
                opcode.set_op(Op::Query);
            }
            RequestOp::Status => {
                hflags.set_recursion_desired(true);

                opcode.set_response(false);
                opcode.set_op(Op::Query);
            }
        }

        // Create the request header
        let header = HeaderBuilder::default()
            .transaction_id(id)
            .opcode(opcode)
            .flags(hflags)
            // TODO: Into would be a good call here in the builder
            .rcode(QueryCode::Success.into())
            .questions(1)
            .additional(1)
            .questions_entries(vec![question])
            .additional_records(vec![resource])
            .build()
            .unwrap();

        // Return the transaction id and the header
        Ok((id, header))
    }
}
