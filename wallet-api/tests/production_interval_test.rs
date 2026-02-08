use tokio::time::Duration;
use wallet_api::infrastructure::runtime::time::new_production_interval;

#[tokio::test]
async fn test_production_interval_barrier() {
    // 创建一个1秒的interval
    let mut interval = new_production_interval(Duration::from_secs(1));

    // 记录开始时间
    let start = std::time::Instant::now();

    // 第一次tick应该立即返回（吃掉即时tick）
    interval.tick().await;

    // 第二次tick应该等待1秒
    interval.tick().await;

    // 验证总共等待了大约1秒
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_secs(1));
    assert!(elapsed < Duration::from_secs(3)); // 放宽时间限制以适应测试环境
}

#[tokio::test]
async fn test_production_interval_basic() {
    // 创建一个100毫秒的interval
    let mut interval = new_production_interval(Duration::from_millis(100));

    // 吃掉第一次即时tick
    interval.tick().await;

    // 记录开始时间
    let start = std::time::Instant::now();

    // 执行一次tick
    interval.tick().await;

    // 验证等待了大约100毫秒
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(50));
    assert!(elapsed < Duration::from_millis(300)); // 放宽时间限制以适应测试环境
}

#[tokio::test]
async fn test_production_interval_zero_duration() {
    // 测试零时长应该触发断言
    let result = std::panic::catch_unwind(|| {
        let _ = new_production_interval(Duration::ZERO);
    });

    // 验证确实触发了panic
    assert!(result.is_err(), "应该触发断言");
}
