//! Project setup commands (port spec, Part C): `c3 init` provisions a repository for the
//! C3 workflow and `c3 doctor` reports what still needs doing. The scout/fetch
//! context-gathering commands live under [`crate::pack::scout`].

pub mod doctor;
pub mod init;
