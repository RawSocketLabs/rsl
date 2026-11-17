//! **sangfor-atrust** — a client for the Sangfor aTrust VPN protocol, written from the
//! clean-room specification `../SPEC-atrust.md`.
//!
//! Built so far: [`policy`], the access policy a gateway hands a logged-in client and the
//! lookups that decide whether a destination goes through the VPN; [`api`], the requests and
//! answers of the gateway's control plane; and [`login`], the sequence that turns a password
//! into a session. The TCP tunnel and the network layer follow.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod api;
pub mod login;
pub mod policy;
