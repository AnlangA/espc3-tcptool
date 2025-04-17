use anyhow::Result;
use esp_idf_hal::prelude::*;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::BlockingWifi,
};
use esp_idf_sys;
use std::time::Duration;



fn main() -> Result<()> {
    // 初始化ESP32
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // 设置WiFi
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let peripherals = Peripherals::take().unwrap();
    let mut wifi = BlockingWifi::wrap(
        esp_idf_svc::wifi::EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;

    // 配置WiFi为STA模式
    let wifi_configuration = wifi.wifi_mut().get_configuration()?;
    wifi.wifi_mut().set_configuration(&wifi_configuration)?;
    wifi.start()?;
    println!("WiFi已启动为STA模式");

    println!("WiFi已准备就绪");

    // 主循环：发送数据
    let mut counter = 0;
    loop {
        counter += 1;
        let message = format!("ESP32数据，计数: {}", counter);

        println!("正在发送: {}", message);

        // 这里可以添加其他通信方式的代码
        // 目前只打印消息到控制台
        println!("消息已准备: {}", message);

        // 等待一秒
        std::thread::sleep(Duration::from_secs(1));
    }

    // 这部分代码不会执行，因为上面有无限循环
    // 但我们保留它作为参考
    #[allow(unreachable_code)]
    Ok(())
}