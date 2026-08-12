//! `task_scheduler`(補助スピン埋め込みによるナップサック問題の東芝SBM
//! 風求解)の実Vulkanデバイス検証(2026-08-12)。
//!
//! CPU参照実装(`sbm_real_vulkan.rs`と同じ設計)ではなく、GPU版
//! (`optimize_task_selection_gpu`)が実機で全探索の最適値に近い解へ
//! 収束することを確認する。

use std::path::PathBuf;

use dream_os_kernel::open_device;
use dream_os_kernel::task_scheduler::optimize_task_selection_gpu;

fn brute_force_best_value(costs: &[u32], values: &[f64], budget: u32) -> f64 {
    let n = costs.len();
    let mut best = 0.0f64;
    for mask in 0u32..(1 << n) {
        let mut cost = 0u32;
        let mut value = 0.0f64;
        for i in 0..n {
            if mask & (1 << i) != 0 {
                cost += costs[i];
                value += values[i];
            }
        }
        if cost <= budget && value > best {
            best = value;
        }
    }
    best
}

#[test]
fn gpu_task_selection_respects_budget_and_approaches_brute_force_optimum_on_real_hardware() {
    let spv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders").join("sbm_ising.spv");
    let spirv = match std::fs::read(&spv_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            eprintln!("skipping: {} not found, compile it first with glslc", spv_path.display());
            return;
        }
    };
    let device = match open_device(0) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("skipping: no real Vulkan device available: {e}");
            return;
        }
    };

    let entries: Vec<(&str, u32, f64)> = vec![
        ("render_pass", 30, 9.0),
        ("mining_batch", 45, 12.0),
        ("flash_attention_prefill", 20, 6.0),
        ("raid_parity_rebuild", 15, 4.0),
        ("sbm_selfcheck", 10, 3.0),
    ];
    let budget = 60u32;
    let costs: Vec<u32> = entries.iter().map(|(_, c, _)| *c).collect();
    let values: Vec<f64> = entries.iter().map(|(_, _, v)| *v).collect();

    let result = optimize_task_selection_gpu(&device, &spirv, &entries, budget, 42, 1200, 0.5).expect("gpu solve failed");
    assert!(result.total_cost <= budget, "selection exceeds budget: {} > {}", result.total_cost, budget);

    let optimal = brute_force_best_value(&costs, &values, budget);
    assert!(
        result.total_value >= optimal * 0.75,
        "gpu sbm result {:.2} is below 75% of brute-force optimum {:.2}",
        result.total_value,
        optimal
    );

    println!(
        "GPU task selection: keep={:?}, defer={:?}, cost={}/{}, value={:.2} (brute-force optimum={:.2}, used_sbm_solution={})",
        result.keep, result.defer, result.total_cost, budget, result.total_value, optimal, result.used_sbm_solution
    );
}
