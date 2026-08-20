//! aruaru-db永続化層(2026-08-06新設)。
//!
//! ユーザー指示「dream-osをopen-directx・open-cuda・open-web-server・
//! RPoem・open-raid-z・aruaru-dbなどをベースに関連技術を実装開発、
//! 完成度と連携性の向上をして」への対応。
//!
//! World Laboratory構想(world-lab/docs/world-laboratory-design.md(https://github.com/aon-co-jp/world-lab))の通信・
//! 永続化層(第4層)のうち、これまで「実DBインスタンスが無いため未配線」
//! と記録していた永続化部分を、`aruaru-db`(ACID互換+Git-on-SQLの
//! バージョン管理、`aruaru-server`が話すPostgreSQL wireプロトコル)へ
//! 実際に接続する形で実装する。`tokio-postgres`クライアントで
//! `aruaru-server`(pgwire)へ接続し、`WorkResultEnvelope`をテーブルへ
//! INSERTした上で`aruaru_commit(...)`を呼び、コミットIDを持つバージョン
//! 管理された結果として永続化する。
//!
//! **認証について(2026-08-20訂正、`aruaru-db`側`aruaru-wire::auth`の
//! 実装を確認の上で修正)**: `aruaru-server`は「既定で認証を要求しない
//! 開発用構成」ではない——`aruaru-wire`は常に`AruaruStartupHandler`
//! (SCRAM-SHA-256の`SASLScramAuthStartupHandler`)をスタートアップ
//! ハンドラとして使う設計であり、認証をスキップする経路自体が存在しない。
//! ユーザーストアは環境変数`ARUARU_USERS`("user1:pass1,user2:pass2")
//! から`AruaruAuthSource::from_env`が読み込む方式で、この環境変数が
//! 未設定の場合は「認証不要になる」のではなく「登録ユーザー0件のため
//! 全てのSCRAMログインが失敗する」(`aruaru-db/crates/aruaru-wire/src/
//! auth.rs`参照)。したがって`AruaruDbStore::connect`を使う際は、
//! 接続先の`aruaru-server`起動時に`ARUARU_USERS`で登録したユーザー名・
//! パスワードを`conn_str`の`user=`/`password=`に指定する必要がある。

use anyhow::{Context, Result};
use base64::Engine;
use tokio_postgres::{Client, NoTls};

use crate::WorkResultEnvelope;

/// `aruaru-server`(pgwire)への接続。
pub struct AruaruDbStore {
    client: Client,
}

impl AruaruDbStore {
    /// `conn_str`例: `"host=127.0.0.1 port=15432 user=dreamos password=<ARUARU_USERSで
    /// 登録した平文パスワード> dbname=aruaru"`(`dbname`は`aruaru-server`側で
    /// 検証されないため任意の値で構わないが、`user`/`password`は接続先
    /// `aruaru-server`起動時の`ARUARU_USERS`環境変数に登録済みの組み合わせで
    /// なければSCRAM-SHA-256認証で拒否される。上記モジュールdoc参照)。
    pub async fn connect(conn_str: &str) -> Result<Self> {
        let (client, connection) = tokio_postgres::connect(conn_str, NoTls).await.context("failed to connect to aruaru-server (pgwire)")?;
        // pgwireクライアントの慣例: 接続処理を別タスクで駆動し続ける必要がある。
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("aruaru-db pgwire connection error: {e}");
            }
        });
        Ok(Self { client })
    }

    /// `world_lab_results`テーブルを用意する(既に存在すれば無視、
    /// `CREATE TABLE IF NOT EXISTS`)。
    pub async fn ensure_schema(&self) -> Result<()> {
        self.client
            .simple_query("CREATE TABLE IF NOT EXISTS world_lab_results (work_unit_id TEXT, worker_id TEXT, result_json TEXT)")
            .await
            .context("failed to create world_lab_results table")?;
        Ok(())
    }

    /// ワークユニットの永続化テーブル(2026-08-20新設)を用意する。
    ///
    /// `world-lab-coordinator`のワークユニット状態(登録内容・claimed
    /// フラグ)を、`aruaru_persistence`側の既存パターン(単純
    /// `simple_query`+手動エスケープ、`aruaru-query`のSQLパーサーの
    /// サブセット制約を踏まえたBase64エンコード)にそのまま従って
    /// 永続化する。`claimed`は真偽値ではなく`'true'`/`'false'`の
    /// テキストとして保持する——`aruaru-query`のUPDATE文パーサーは
    /// `SET col = 'val'`の単純な文字列リテラルしか受け付けないため。
    pub async fn ensure_work_units_schema(&self) -> Result<()> {
        self.client
            .simple_query("CREATE TABLE IF NOT EXISTS world_lab_work_units (id TEXT, payload_b64 TEXT, claimed TEXT)")
            .await
            .context("failed to create world_lab_work_units table")?;
        Ok(())
    }

    /// 新規ワークユニットを1件挿入する(`payload`はカンマ等を含みうる
    /// JSONのため、`persist_result`と同じ理由でBase64エンコードして
    /// 格納する)。
    pub async fn insert_work_unit(&self, id: &str, payload: &serde_json::Value, claimed: bool) -> Result<()> {
        let payload_json = serde_json::to_string(payload).context("failed to serialize work unit payload")?;
        let payload_b64 = base64::engine::general_purpose::STANDARD.encode(payload_json.as_bytes());
        let sql = format!(
            "INSERT INTO world_lab_work_units (id, payload_b64, claimed) VALUES ('{}', '{}', '{}')",
            escape_sql_literal(id),
            payload_b64,
            if claimed { "true" } else { "false" },
        );
        self.client.simple_query(&sql).await.context("failed to insert work unit")?;
        Ok(())
    }

    /// 既存ワークユニットの`claimed`フラグを更新する。
    pub async fn update_work_unit_claimed(&self, id: &str, claimed: bool) -> Result<()> {
        let sql = format!(
            "UPDATE world_lab_work_units SET claimed = '{}' WHERE id = '{}'",
            if claimed { "true" } else { "false" },
            escape_sql_literal(id),
        );
        self.client.simple_query(&sql).await.context("failed to update work unit claimed flag")?;
        Ok(())
    }

    /// 全ワークユニットを`(id, payload, claimed)`のVecとして読み出す
    /// (コーディネータ起動時の`AppState`復元用)。挿入順は保証されない
    /// ため、呼び出し側で`id`(`wu-NNNNNN`という単調増加の連番文字列)
    /// によるソートを行うことを想定する。
    pub async fn load_all_work_units(&self) -> Result<Vec<(String, serde_json::Value, bool)>> {
        let rows = self
            .client
            .simple_query("SELECT id, payload_b64, claimed FROM world_lab_work_units")
            .await
            .context("failed to load work units")?;

        let mut out = Vec::new();
        for msg in rows {
            if let tokio_postgres::SimpleQueryMessage::Row(row) = msg {
                let id = row.get(0).unwrap_or_default().to_string();
                let payload_b64 = row.get(1).unwrap_or_default();
                let claimed_str = row.get(2).unwrap_or_default();

                let payload_json_bytes = base64::engine::general_purpose::STANDARD
                    .decode(payload_b64)
                    .context("failed to base64-decode stored work unit payload")?;
                let payload: serde_json::Value =
                    serde_json::from_slice(&payload_json_bytes).context("failed to deserialize stored work unit payload")?;
                let claimed = claimed_str == "true";

                out.push((id, payload, claimed));
            }
        }
        Ok(out)
    }

    /// `WorkResultEnvelope`を1行INSERTし、`aruaru_commit`でコミットして
    /// commit_idを返す(ACID互換+Git版管理の両方を実際に活用する経路)。
    ///
    /// **実機検証で発見した実際の制約(正直な開示)**: `aruaru-query`の
    /// SQLパーサー(`parser.rs::parse_insert`)は`VALUES(...)`内の値を
    /// クォートを考慮しない単純な`split(',')`で分割する簡易実装
    /// (フルSQL構文パーサーではなく、意図的に絞り込まれたサブセット)。
    /// そのため`result_json`(JSON文字列)にカンマが含まれると
    /// `"INSERT: N columns but M values"`エラーになることを実際の
    /// `aruaru-server`インスタンスへの接続で発見した。回避策として
    /// `result_json`をBase64エンコードしてから格納する(カンマ・
    /// クォート等パーサーが誤分割しうる文字を含まない安全な文字集合の
    /// みになる)。
    pub async fn persist_result(&self, envelope: &WorkResultEnvelope) -> Result<String> {
        let result_json = serde_json::to_string(&envelope.result_json).context("failed to serialize result_json")?;
        let result_json_b64 = base64::engine::general_purpose::STANDARD.encode(result_json.as_bytes());

        let insert_sql = format!(
            "INSERT INTO world_lab_results (work_unit_id, worker_id, result_json) VALUES ('{}', '{}', '{}')",
            escape_sql_literal(&envelope.work_unit_id),
            escape_sql_literal(&envelope.worker_id),
            result_json_b64,
        );
        self.client.simple_query(&insert_sql).await.context("failed to insert world lab result")?;

        let commit_msg = format!("world laboratory result: {}", envelope.work_unit_id);
        let commit_sql = format!("SELECT aruaru_commit('{}')", escape_sql_literal(&commit_msg));
        let rows = self.client.simple_query(&commit_sql).await.context("failed to run aruaru_commit")?;

        for msg in rows {
            if let tokio_postgres::SimpleQueryMessage::Row(row) = msg {
                if let Some(commit_id) = row.get(0) {
                    return Ok(commit_id.to_string());
                }
            }
        }
        anyhow::bail!("aruaru_commit did not return a commit_id")
    }
}

/// 単純な文字列エスケープ(単一引用符の重複、`aruaru-wire`が持つ
/// SQLインジェクション対策と同じ発想の最小限の実装——本来はプリペアド
/// ステートメントを使うべきだが、`aruaru-wire`の拡張プロトコル
/// 対応状況〈`describe_portal`が動的スキーマのため空列を返す既知の
/// 制約、`aruaru-db/CLAUDE.md`参照〉を踏まえ、既存のこのエコシステムの
/// 他クライアントコードと同じ`simple_query`+手動エスケープ方式に揃えた)。
fn escape_sql_literal(s: &str) -> String {
    s.replace('\'', "''")
}
