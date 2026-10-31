//! Privchat 协议适配层：透传 privchat-sdk，不做 DTO 转换。
//!
//! 实现 AuthPort、ChatDataPort、EventStreamPort，供 App/Backend 使用。

mod service;

pub use service::PrivchatService;
