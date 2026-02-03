// 应用模块
// App Module

use std::sync::Arc;
use tokio::sync::Mutex;

pub mod hosts;
pub mod host_groups;
pub mod host_execute;

// 重新导出 AppState 让子模块可以使用
pub use super::AppState;

// 公共类型别名
pub type SharedAppState = Arc<AppState>;
pub type LockedConfig = Mutex<super::Config>;
