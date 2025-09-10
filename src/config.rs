// src/config.rs
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MonitorConfig {
    #[serde(default)]
    pub strategy_a: Vec<StrategyARule>,
    #[serde(default)]
    pub strategy_b: Vec<StrategyBRule>, // 新增
    #[serde(default)]
    pub strategy_c: Vec<StrategyCRule>,
}

#[derive(Debug, Deserialize)]
pub struct StrategyARule {
    pub file: String,
    pub functions: Vec<String>,
}

// 新增 StrategyBRule 结构体
#[derive(Debug, Deserialize)]
pub struct StrategyBRule {
    pub file: String,
    pub functions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StrategyCRule {
    pub file: String,
    pub traits: Vec<String>,
}