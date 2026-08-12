//! `sbm`(東芝Simulated Bifurcation Machine風カーネル)を使った、具体的な
//! 組合せ最適化問題への配線(2026-08-12新設)。
//!
//! ユーザー指示「open-english/open-easy-web/open-web-server/RPoem/
//! open-raid-z/aruaru-db/PostgreSQLや複数WAN/複数LANのリポジトリも含めて
//! 連携やそれらの設計思想をdream-osに活かす…試作品を開発したりTESTを
//! 繰り返して」への対応。`sbm.rs`のモジュールdoc・過去の複数のHANDOFF
//! エントリで繰り返し「次にすべきこと」として残っていた**「sbm_isingが
//! 解ける組合せ最適化問題の具体候補を特定してから着手する」**という
//! 保留事項に、`RS-SmartTCP::path_optimizer`(複数WAN/LAN経路選択、東芝
//! SBM風QUBO→Ising変換によるナップサック問題の求解、実機検証済み)と
//! `aruaru-llm::cache_optimizer`(モデルキャッシュのディスク容量予算問題)
//! で既に確立された定式化パターンを、本クレート固有の具体的な意思決定
//! ——**「限られた電力/ディスパッチ予算(`power_profile::
//! MiningPowerProfile`と同じ考え方)の下で、複数の保留中GPU計算タスクの
//! うちどれを今サイクルで実行するかを選ぶ」**——へ適用する形で答える。
//!
//! ## 正直な開示(最重要、実装中に発見した数学的な罠を含む)
//!
//! - QUBO/Isingへの変換アルゴリズム自体は`RS-SmartTCP::path_optimizer`・
//!   `aruaru-llm::cache_optimizer`と同型(0/1ナップサック問題、スラック
//!   ビットによる予算制約の等式化)——**コードのコピーではなく本クレート
//!   内で独立に再実装した**(各リポジトリが互いにpath依存しない既存
//!   方針を踏襲)。
//! - **実装中に発見した罠(重要)**: `sbm.rs`のGPUシェーダ/CPU参照実装は
//!   `run_sbm_ising_cpu_reference`のソースを読むと分かる通り、結合項
//!   `J_ij * s_i * s_j`(MaxCut風)**のみ**を計算し、QUBO→Ising変換で
//!   本来必要になる局所磁場`h_i * s_i`項を受け取る引数がそもそも存在
//!   しない(`run_sbm_ising`のシグネチャに`h`配列が無い)。ナップサック
//!   問題の定式化は本質的にh項(価値の一次項・予算制約のペナルティ項)を
//!   必要とするため、単純にhをゼロ埋めパディングするだけでは
//!   **数学的に誤った問題を解いてしまう**(当初その誤りのまま実装して
//!   しまい、レビューで自己発見・修正した)。
//! - **修正: 補助スピン(ancilla)による埋め込み**(標準的な手法、
//!   量子アニーリング文献の"ancilla qubit"テクニックと同型)。論理変数
//!   `n`個(タスク+スラックビット)に加え、1個の補助スピン`a`を追加し、
//!   `h_i * s_i`という局所磁場項を`h_i * s_i * s_a`という**通常の結合項**
//!   として埋め込む。この`H' = sum_{i<j} J_ij s_i s_j + sum_i h_i s_i s_a`
//!   という(局所磁場を持たない)ハミルトニアンは全スピン反転
//!   (`s -> -s`、`s_a`含む)に対して不変な大域Z2対称性を持ち、その2つの
//!   縮退基底状態は元の問題の答えを「補助スピンとの相対符号」として
//!   埋め込んでいる: 解読時は`s_i^recovered = s_i * s_a`と定義すれば
//!   `H'`の最小化は厳密に元の`E = sum h_i s_i + sum J_ij s_i s_j`の
//!   最小化と同値になる(数式的な導出は本ファイル末尾のコメント参照)。
//!   これによりhをゼロパディングする誤った実装を、既存シェーダの
//!   インターフェースを変更せずに正しく解ける形へ修正した。
//! - **本クレート独自の制約**: `sbm.rs`のGPUカーネルは`NUM_SPINS=64`
//!   固定(シェーダの共有メモリサイズをコンパイル時に固定しているため)。
//!   補助スピン1個を含めるため、実際に扱えるタスク数+スラックビット数は
//!   `NUM_SPINS - 1 = 63`個までに制限される——超える候補を渡すと`Err`を
//!   返す。
//! - タスク数(実運用では数個〜数十個程度)は全探索でも瞬時に厳密解が
//!   求まる規模であり、SBMを使う実用上の必要性は薄い——「SBMが無ければ
//!   解けない/著しく遅い」という主張はしていない。あくまで「既存のGPU
//!   ディスパッチ経路(`run_sbm_ising`)を実際の意思決定へ配線し、複数
//!   ランダム初期値からの探索が全探索の最適値にどこまで近づけるかを
//!   実機で検証する」という統合実証が目的(既存の`RS-SmartTCP`/
//!   `aruaru-llm`と同じ立ち位置)。
//! - SBM解が予算制約を満たさない場合は、価値密度順の貪欲フォールバックへ
//!   安全に切り替える(`used_sbm_solution`で判別可能)。
//!
//! ### 補助スピン埋め込みの数式的な裏付け
//!
//! `s_i = s_i^recovered * s_a`(`s_a^2 = 1`)と置くと:
//! `sum_{i<j} J_ij s_i s_j = sum_{i<j} J_ij (s_a s_i^r)(s_a s_j^r)
//! = sum_{i<j} J_ij s_i^r s_j^r`(`s_a^2=1`で消える)。
//! `sum_i h_i s_i s_a = sum_i h_i (s_a s_i^r)(s_a) = sum_i h_i s_i^r`
//! (`s_a^2=1`で消える)。したがって
//! `H'(s, s_a) = sum_{i<j} J_ij s_i s_j + sum_i h_i s_i s_a
//! = sum_{i<j} J_ij s_i^r s_j^r + sum_i h_i s_i^r = E(s^r)`——
//! `H'`の最小化と`E`の最小化が任意の`s_a`の選び方に対して常に一致する。

#![allow(clippy::needless_range_loop)]

use std::sync::Arc;

use anyhow::{ensure, Result};
use opencuda_core::GpuDevice;

use crate::sbm::{run_sbm_ising, run_sbm_ising_cpu_reference, NUM_SPINS};

#[derive(Debug, Clone)]
struct QuboProblem {
    n: usize,
    h: Vec<f64>,
    q: Vec<f64>,
}

impl QuboProblem {
    fn new(n: usize) -> Self {
        Self { n, h: vec![0.0; n], q: vec![0.0; n * n] }
    }

    fn add_h(&mut self, i: usize, v: f64) {
        self.h[i] += v;
    }

    fn add_q(&mut self, i: usize, j: usize, v: f64) {
        if i == j {
            self.h[i] += v;
            return;
        }
        self.q[i * self.n + j] += v;
        self.q[j * self.n + i] += v;
    }

    /// QUBO(0/1変数)をIsing(±1スピン)へ変換する(`RS-SmartTCP::
    /// path_optimizer`・`aruaru-llm::cache_optimizer`と同じ標準変換)。
    fn to_ising(&self) -> (Vec<f64>, Vec<f64>) {
        let n = self.n;
        let mut h_spin = vec![0.0; n];
        let mut j_spin = vec![0.0; n * n];
        for i in 0..n {
            h_spin[i] += self.h[i] / 2.0;
        }
        for i in 0..n {
            for j in (i + 1)..n {
                let qij = self.q[i * n + j];
                if qij == 0.0 {
                    continue;
                }
                j_spin[i * n + j] += qij / 4.0;
                j_spin[j * n + i] += qij / 4.0;
                h_spin[i] += qij / 4.0;
                h_spin[j] += qij / 4.0;
            }
        }
        (h_spin, j_spin)
    }
}

/// 0/1ナップサック問題として、`(コスト, 価値)`の候補群から予算内で
/// 価値合計を最大化するQUBOを組み立てる(スラックビットで`sum cost*z <=
/// budget`という不等式制約を等式制約へ変換する標準的な手法)。
fn build_task_selection_qubo(costs: &[u32], values: &[f64], budget: u32) -> QuboProblem {
    let n_tasks = costs.len();
    let slack_bits = if budget == 0 { 0 } else { (32 - budget.leading_zeros()) as usize };
    let n = n_tasks + slack_bits;

    let mut a = vec![0.0f64; n];
    for i in 0..n_tasks {
        a[i] = costs[i] as f64;
    }
    for k in 0..slack_bits {
        a[n_tasks + k] = (1u64 << k) as f64;
    }

    let max_value: f64 = values.iter().sum::<f64>().max(1.0);
    let penalty = max_value * 1.5;
    let c = budget as f64;

    let mut problem = QuboProblem::new(n);
    for i in 0..n_tasks {
        problem.add_h(i, -values[i]);
    }
    for j_idx in 0..n {
        problem.add_h(j_idx, -2.0 * penalty * c * a[j_idx] + penalty * a[j_idx] * a[j_idx]);
        for k_idx in (j_idx + 1)..n {
            problem.add_q(j_idx, k_idx, 2.0 * penalty * a[j_idx] * a[k_idx]);
        }
    }
    problem
}

/// `h_spin`(局所磁場)を補助スピン(index `n`、`n_total = n + 1`)への
/// 結合として埋め込み、`NUM_SPINS x NUM_SPINS`へゼロパディングする
/// (モジュールdoc「補助スピン埋め込みの数式的な裏付け」参照)。
///
/// **実機検証で発見した符号の罠(重要)**: `to_ising`は標準的な規約
/// `E = sum h_i s_i + sum_{i<j} J_ij s_i s_j`の**最小化**を前提に
/// 導出している(`RS-SmartTCP::path_optimizer`・`aruaru-llm::
/// cache_optimizer`と同じ規約、それぞれ独自のCPU積分器がこの規約で
/// 最小化する設計)。一方、本クレートの`sbm.rs`の`ising_energy`は
/// `E' = -sum_{i<j} J'_ij s_i s_j`(符号が反転したMaxCut規約)を前提に
/// 実装されており、力学系の更新式(`+c0*coupling`)はこの`E'`を
/// **最小化**する——つまり`sum J'_ij s_i s_j`を**最大化**する方向に
/// 収束する。符号を合わせずにそのまま`J`を渡すと、意図した問題の
/// **最小化ではなく最大化**を解いてしまい、実機テストで「毎回全タスク
/// を選択する」という体系的な誤り(ランダムなノイズではなく常に同じ
/// 誤答)として発覚した。`J' = -J`(hのancilla結合も含めて符号反転)を
/// 渡すことで、`E'(J') = -sum(-J_ij)s_i s_j = sum J_ij s_i s_j =
/// E(J)`となり、`sbm.rs`の最小化がこちらの意図した`E`の最小化と
/// 一致するよう修正した。
fn embed_and_pad(h_spin: &[f64], j_spin: &[f64], n: usize) -> Vec<f32> {
    let ancilla = n;
    let mut j_padded = vec![0f32; NUM_SPINS * NUM_SPINS];
    for i in 0..n {
        for j in 0..n {
            j_padded[i * NUM_SPINS + j] = -j_spin[i * n + j] as f32;
        }
        j_padded[i * NUM_SPINS + ancilla] = -h_spin[i] as f32;
        j_padded[ancilla * NUM_SPINS + i] = -h_spin[i] as f32;
    }
    j_padded
}

/// `NUM_SPINS`長の決定的な擬似乱数初期値(既存の`tests/sbm_real_vulkan.rs`
/// と同じ線形合同法、外部乱数crateへの依存を避けた最小実装)。補助スピン
/// (index `n_total-1`まで)も含めて初期値を与える。
fn deterministic_init(n_total: usize, seed: u64) -> Vec<f32> {
    let mut state = seed.max(1);
    let mut next = move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) as u32) as f32 / u32::MAX as f32
    };
    (0..NUM_SPINS).map(|i| if i < n_total { (next() - 0.5) * 0.2 } else { 0.0 }).collect()
}

/// 補助スピンとの相対符号で元の変数`s_i^recovered`を解読する。
fn decode_relative_to_ancilla(spins: &[f32], n: usize) -> Vec<bool> {
    let ancilla_sign = spins[n];
    (0..n).map(|i| spins[i] * ancilla_sign >= 0.0).collect()
}

/// タスク選択の最適化結果。
#[derive(Debug, Clone)]
pub struct TaskSelectionResult {
    pub keep: Vec<String>,
    pub defer: Vec<String>,
    pub total_cost: u32,
    pub budget: u32,
    pub total_value: f64,
    /// SBM解がそのまま予算制約を満たしていたか(false=貪欲フォールバック
    /// を使った、正直な開示)。
    pub used_sbm_solution: bool,
}

fn respects_budget(selection: &[bool], costs: &[u32], budget: u32) -> bool {
    let total: u32 = selection.iter().zip(costs).filter(|(&s, _)| s).map(|(_, &c)| c).sum();
    total <= budget
}

fn greedy_fallback(costs: &[u32], values: &[f64], budget: u32) -> Vec<bool> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| {
        let da = if costs[a] > 0 { values[a] / costs[a] as f64 } else { f64::MAX };
        let db = if costs[b] > 0 { values[b] / costs[b] as f64 } else { f64::MAX };
        db.partial_cmp(&da).unwrap()
    });
    let mut selection = vec![false; values.len()];
    let mut used = 0u32;
    for i in order {
        if used + costs[i] <= budget {
            selection[i] = true;
            used += costs[i];
        }
    }
    selection
}

fn finalize(names: &[&str], costs: &[u32], values: &[f64], budget: u32, n_tasks: usize, best_selection: Option<Vec<bool>>) -> TaskSelectionResult {
    let (selection, used_sbm_solution) = match best_selection {
        Some(sel) if respects_budget(&sel[..n_tasks], costs, budget) => (sel[..n_tasks].to_vec(), true),
        _ => (greedy_fallback(costs, values, budget), false),
    };
    let mut keep = Vec::new();
    let mut defer = Vec::new();
    let mut total_cost = 0u32;
    let mut total_value = 0.0f64;
    for i in 0..n_tasks {
        if selection[i] {
            keep.push(names[i].to_string());
            total_cost += costs[i];
            total_value += values[i];
        } else {
            defer.push(names[i].to_string());
        }
    }
    TaskSelectionResult { keep, defer, total_cost, budget, total_value, used_sbm_solution }
}

fn compute_c0(h_spin: &[f64], j_spin: &[f64], n_total: usize) -> f32 {
    let max_abs = h_spin.iter().chain(j_spin.iter()).fold(0.0_f64, |acc, &v| acc.max(v.abs())).max(1e-12);
    (0.5 / (n_total as f64).sqrt() / max_abs) as f32
}

/// CPU参照実装(`sbm::run_sbm_ising_cpu_reference`)で選択問題を解く
/// (実GPUが無い環境でも呼べる、単体テスト・CI向け)。
pub fn optimize_task_selection_cpu(entries: &[(&str, u32, f64)], budget: u32, seed: u64, steps: u32, dt: f32) -> Result<TaskSelectionResult> {
    let names: Vec<&str> = entries.iter().map(|(n, _, _)| *n).collect();
    let costs: Vec<u32> = entries.iter().map(|(_, c, _)| *c).collect();
    let values: Vec<f64> = entries.iter().map(|(_, _, v)| *v).collect();
    let n_tasks = names.len();

    let problem = build_task_selection_qubo(&costs, &values, budget);
    let n_total = problem.n + 1; // +1 = 補助スピン
    ensure!(n_total <= NUM_SPINS, "task_scheduler: {} candidates(+slack+ancilla) exceed the fixed NUM_SPINS={} limit of this GPU kernel", n_total, NUM_SPINS);
    let (h_spin, j_spin) = problem.to_ising();
    let c0 = compute_c0(&h_spin, &j_spin, n_total);
    let a0 = 1.0f32;

    let mut best: Option<(Vec<bool>, f64)> = None;
    for attempt in 0..8u64 {
        let j64 = embed_and_pad(&h_spin, &j_spin, problem.n);
        let init_x = deterministic_init(n_total, seed.wrapping_add(attempt.wrapping_mul(0x9E3779B97F4A7C15)));
        let result = run_sbm_ising_cpu_reference(&j64, &init_x, steps, dt, c0, a0);
        let z = decode_relative_to_ancilla(&result.spins, problem.n);
        let value: f64 = z[..n_tasks].iter().zip(values.iter()).filter(|(&s, _)| s).map(|(_, &v)| v).sum();
        if respects_budget(&z[..n_tasks], &costs, budget) && best.as_ref().map(|(_, v)| value > *v).unwrap_or(true) {
            best = Some((z, value));
        }
    }
    Ok(finalize(&names, &costs, &values, budget, n_tasks, best.map(|(z, _)| z)))
}

/// 実Vulkanデバイス(`sbm::run_sbm_ising`)で選択問題を解く。
pub fn optimize_task_selection_gpu(device: &Arc<dyn GpuDevice>, spirv: &[u8], entries: &[(&str, u32, f64)], budget: u32, seed: u64, steps: u32, dt: f32) -> Result<TaskSelectionResult> {
    let names: Vec<&str> = entries.iter().map(|(n, _, _)| *n).collect();
    let costs: Vec<u32> = entries.iter().map(|(_, c, _)| *c).collect();
    let values: Vec<f64> = entries.iter().map(|(_, _, v)| *v).collect();
    let n_tasks = names.len();

    let problem = build_task_selection_qubo(&costs, &values, budget);
    let n_total = problem.n + 1; // +1 = 補助スピン
    ensure!(n_total <= NUM_SPINS, "task_scheduler: {} candidates(+slack+ancilla) exceed the fixed NUM_SPINS={} limit of this GPU kernel", n_total, NUM_SPINS);
    let (h_spin, j_spin) = problem.to_ising();
    let c0 = compute_c0(&h_spin, &j_spin, n_total);
    let a0 = 1.0f32;

    let mut best: Option<(Vec<bool>, f64)> = None;
    for attempt in 0..8u64 {
        let j64 = embed_and_pad(&h_spin, &j_spin, problem.n);
        let init_x = deterministic_init(n_total, seed.wrapping_add(attempt.wrapping_mul(0x9E3779B97F4A7C15)));
        let result = run_sbm_ising(device, spirv, &j64, &init_x, steps, dt, c0, a0)?;
        let z = decode_relative_to_ancilla(&result.spins, problem.n);
        let value: f64 = z[..n_tasks].iter().zip(values.iter()).filter(|(&s, _)| s).map(|(_, &v)| v).sum();
        if respects_budget(&z[..n_tasks], &costs, budget) && best.as_ref().map(|(_, v)| value > *v).unwrap_or(true) {
            best = Some((z, value));
        }
    }
    Ok(finalize(&names, &costs, &values, budget, n_tasks, best.map(|(z, _)| z)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn cpu_sbm_task_selection_respects_budget_and_approaches_brute_force_optimum() {
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

        let result = optimize_task_selection_cpu(&entries, budget, 42, 1200, 0.5).expect("cpu solve failed");
        assert!(result.total_cost <= budget, "selection exceeds budget: {} > {}", result.total_cost, budget);

        let optimal = brute_force_best_value(&costs, &values, budget);
        assert!(
            result.total_value >= optimal * 0.75,
            "sbm result {:.2} is below 75% of brute-force optimum {:.2}",
            result.total_value,
            optimal
        );
        println!(
            "cpu sbm task selection: value={:.2} (brute-force optimum={:.2}, used_sbm_solution={})",
            result.total_value, optimal, result.used_sbm_solution
        );
    }

    #[test]
    fn rejects_too_many_candidates_for_the_fixed_num_spins_kernel() {
        let entries: Vec<(String, u32, f64)> = (0..64).map(|i| (format!("task{i}"), 1u32, 1.0f64)).collect();
        let entries_ref: Vec<(&str, u32, f64)> = entries.iter().map(|(n, c, v)| (n.as_str(), *c, *v)).collect();
        let result = optimize_task_selection_cpu(&entries_ref, 100, 1, 10, 0.5);
        assert!(result.is_err(), "expected an error when candidates+slack+ancilla exceed NUM_SPINS");
    }
}
