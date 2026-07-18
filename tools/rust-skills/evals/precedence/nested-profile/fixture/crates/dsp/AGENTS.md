# DSP subtree instructions

- This crate is a synchronous reusable library.
- The base API must not depend on or own Tokio or Rayon.
- Optional runtime adapters require a separate owner decision.
- Do not add dependencies in this task.
