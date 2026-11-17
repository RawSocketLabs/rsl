//! **sangfor-atrust** — a client for the Sangfor aTrust VPN protocol, written from the
//! clean-room specification `../SPEC-atrust.md`.
//!
//! Built so far: [`policy`], the access policy a gateway hands a logged-in client and the
//! lookups that decide whether a destination goes through the VPN; [`api`], the requests and
//! answers of the gateway's control plane; [`login`], the sequence that turns a password
//! into a session; and [`tcp`], one TCP stream through a node. The network layer follows.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod api;
pub mod login;
pub mod policy;
pub mod tcp;
