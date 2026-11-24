//! The wallet session's private claim and burn as jobs: started by one call, run in a thread of the module, polled.
//!
//! Logos Basecamp (its pin `b642a9a`: logos-view-module-runtime `3e69973` over logos-protocol `9d59f5d`) ends a
//! view's call to a core module after 20 s whatever the view asks (`modules/ferrata_bridge_ui/
//! results-basecamp-roundtrip/README.md`, "L4"), and a private claim or burn proven on a CPU takes about four
//! minutes. So the view's `claim` and `burn` start a job here and are answered at once with the job's record;
//! `job {"id"}` answers its state (`queued`, `proving`, `sent`, `settled`, `unconfirmed` or `failed`), the time it has
//! taken and,
//! once it is done, exactly the reply the call used to carry (`result`) or its error; `jobs` lists every job the
//! module has run since it started, so that a view opened again, or one that missed a reply, finds them. The jobs
//! live in the module's memory (the module keeps no state on disk) for as long as the module runs.
//!
//! The job's thread reaches `lez_core` through the Logos SDK, which runs each call on the module's Qt thread
//! (logos-protocol's `runOnOwnerThread`, a blocking queued call) and waits there for the reply in a nested event loop,
//! during which the Qt thread goes on answering the view's polls. That needs the Qt thread free, so the method that
//! started the job returns first (docs/EVIDENCE.md row 121: a module whose method blocks its own Qt thread cannot call
//! another module from a worker thread). For the same reason, while a job runs the session refuses at once the calls
//! that would call `lez_core` or reread the chains on the Qt thread (`session.rs`, `dispatch`): such a call would wait
//! there for `lez_core`, busy proving, and the module's dispatch would be entered again from that wait.
//!
//! `{"wait": true}` in the parameters runs the job inside the call itself and answers as the call always did (for a
//! host whose calls may last minutes, such as logosctl with `--timeout`; never inside Basecamp).

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// How many jobs the module keeps: past it, the oldest finished ones are dropped.
const KEEP: usize = 64;

struct Job {
    id: u64,
    kind: String,
    params: Value,
    state: &'static str,
    created_ms: u64,
    finished_ms: Option<u64>,
    phases: Vec<Value>,
    tx_hash: Option<String>,
    result: Option<Value>,
    error: Option<String>,
    inline: bool,
}

struct Jobs {
    next: u64,
    list: Vec<Job>,
    since_ms: u64,
}

static JOBS: Mutex<Jobs> = Mutex::new(Jobs { next: 1, list: Vec::new(), since_ms: 0 });

thread_local! {
    /// The job the current thread runs (0: none).
    static CURRENT: Cell<u64> = const { Cell::new(0) };
}

/// The wall clock in milliseconds since 1970.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The store's lock, held only to read or write a record: never across a call of another module or a chain.
fn lock() -> MutexGuard<'static, Jobs> {
    JOBS.lock().unwrap_or_else(|p| p.into_inner())
}

fn running(state: &str) -> bool {
    !matches!(state, "settled" | "failed" | "unconfirmed")
}

impl Job {
    fn at(&self, state: &str) -> Option<u64> {
        self.phases.iter().find(|p| p["state"] == state).and_then(|p| p["at_ms"].as_u64())
    }

    fn record(&self, now: u64) -> Value {
        let end = self.finished_ms.unwrap_or(now);
        let proving_ms = self.at("proving").map(|t| self.at("sent").unwrap_or(end).saturating_sub(t));
        let run_by = if self.inline {
            "the call itself (wait: true)"
        } else {
            "a thread of ferrata_bridge_module (its Qt thread stays free for the view's calls)"
        };
        json!({
            "id": self.id,
            "kind": self.kind,
            "params": self.params,
            "state": self.state,
            "running": running(self.state),
            "created_ms": self.created_ms,
            "finished_ms": self.finished_ms,
            "elapsed_ms": end.saturating_sub(self.created_ms),
            "proving_ms": proving_ms,
            "phases": self.phases,
            "tx_hash": self.tx_hash,
            "result": self.result,
            "error": self.error,
            "run_by": run_by,
        })
    }
}

/// The job the current thread runs moves on to `state` (`proving`, `sent`), with what it does (`tx_hash` once sent).
/// Outside a job, nothing.
pub(crate) fn phase(state: &'static str, detail: Value) {
    let id = CURRENT.with(Cell::get);
    if id == 0 {
        return;
    }
    let mut g = lock();
    if let Some(j) = g.list.iter_mut().find(|j| j.id == id) {
        if let Some(h) = detail["tx_hash"].as_str().filter(|h| !h.is_empty()) {
            j.tx_hash = Some(h.to_string());
        }
        j.state = state;
        j.phases.push(json!({ "state": state, "at_ms": now_ms(), "detail": detail }));
    }
}

/// The record of the job that is running, if one is.
/// Forgets every job's record (a network session closed: its amounts, recipients and transactions are not kept for the
/// next caller). A job still running finishes, unrecorded.
pub(crate) fn forget() {
    lock().list.clear();
}

pub(crate) fn active() -> Option<Value> {
    let g = lock();
    let now = now_ms();
    g.list.iter().find(|j| running(j.state)).map(|j| j.record(now))
}

/// Why `method` is refused while `job` runs.
pub(crate) fn busy(method: &str, job: &Value) -> String {
    format!(
        "busy: job {} (the private {}) is {} since {} s; `{method}` would wait for it (it calls lez_core or rereads the chains): \
         poll `job` and call it again once the job is done",
        job["id"],
        job["kind"].as_str().unwrap_or("?"),
        job["state"].as_str().unwrap_or("?"),
        job["elapsed_ms"].as_u64().unwrap_or(0) / 1000
    )
}

/// Starts the job `kind` (`claim`, `burn`) with the view's `params`: `step` runs in a thread of the module and the
/// call is answered at once (`{"job": record, "started": true}`), or, with `{"wait": true}`, runs here and the call is
/// answered with the step's own reply. While a job runs, the same step started again answers that job (`"started":
/// false`) and another step is refused: one private transaction at a time (the wallet's LEZ account and its state).
pub(crate) fn start<F>(kind: &str, params: &Value, step: F) -> Result<Value, String>
where
    F: FnOnce(&Value) -> Result<Value, String> + Send + 'static,
{
    let inline = params["wait"].as_bool() == Some(true);
    let now = now_ms();
    let id = {
        let mut g = lock();
        if g.since_ms == 0 {
            g.since_ms = now;
        }
        if let Some(j) = g.list.iter().find(|j| running(j.state)) {
            if j.kind == kind {
                return Ok(json!({
                    "job": j.record(now),
                    "started": false,
                    "note": format!("job {} (the private {}) is already running: picked up, not started again", j.id, j.kind),
                }));
            }
            return Err(format!(
                "refused: job {} (the private {}) is {}: one private transaction at a time (the wallet's LEZ account); start this once it is done",
                j.id, j.kind, j.state
            ));
        }
        let id = g.next;
        g.next += 1;
        g.list.push(Job {
            id,
            kind: kind.to_string(),
            params: params.clone(),
            state: "queued",
            created_ms: now,
            finished_ms: None,
            phases: vec![json!({ "state": "queued", "at_ms": now, "detail": { "doing": "reading the chains, building the transaction" } })],
            tx_hash: None,
            result: None,
            error: None,
            inline,
        });
        while g.list.len() > KEEP {
            match g.list.iter().position(|j| !running(j.state)) {
                Some(i) => {
                    g.list.remove(i);
                }
                None => break,
            }
        }
        id
    };
    let p = params.clone();
    let body = move || {
        CURRENT.with(|c| c.set(id));
        let outcome = catch_unwind(AssertUnwindSafe(|| step(&p))).unwrap_or_else(|_| Err("the job's step panicked".to_string()));
        CURRENT.with(|c| c.set(0));
        finish(id, outcome);
    };
    if inline {
        body();
        let g = lock();
        let j = g.list.iter().find(|j| j.id == id).ok_or("the job's record was dropped")?;
        return match (&j.result, &j.error) {
            (Some(r), _) => {
                let mut r = r.clone();
                r["job"] = json!(id);
                Ok(r)
            }
            (None, e) => Err(e.clone().unwrap_or_else(|| "the job ended without a reply".into())),
        };
    }
    if let Err(e) = std::thread::Builder::new().name(format!("ferrata-job-{id}")).spawn(body) {
        finish(id, Err(format!("no thread for the job: {e}")));
    }
    let g = lock();
    let record = g.list.iter().find(|j| j.id == id).map(|j| j.record(now_ms())).ok_or("the job's record was dropped")?;
    Ok(json!({ "job": record, "started": true }))
}

/// The step's outcome: `settled` when it settled; `unconfirmed` when its transaction was broadcast and not yet seen
/// mined (`pending: true`: it may still be mined, so it is not a failure, and the reply says not to send it again);
/// `failed` with its error otherwise (a reply that did not settle is kept as the result, and the job is `failed`).
fn finish(id: u64, outcome: Result<Value, String>) {
    let mut g = lock();
    if let Some(j) = g.list.iter_mut().find(|j| j.id == id) {
        let now = now_ms();
        j.finished_ms = Some(now);
        match outcome {
            Ok(v) => {
                if let Some(h) = v["tx_hash"].as_str().filter(|h| !h.is_empty()) {
                    j.tx_hash = Some(h.to_string());
                }
                if v["settled"] == true {
                    j.state = "settled";
                } else if v["pending"] == true {
                    j.state = "unconfirmed";
                } else {
                    j.state = "failed";
                    j.error = Some(format!("not settled: {}", v["what"].as_str().unwrap_or("the transaction was not applied")));
                }
                j.result = Some(v);
            }
            Err(e) => {
                j.state = "failed";
                j.error = Some(e);
            }
        }
        let state = j.state;
        j.phases.push(json!({ "state": state, "at_ms": now }));
    }
}

/// The transactions of the `kind` jobs that ended `unconfirmed` (broadcast, not yet seen mined), newest first.
pub(crate) fn unconfirmed_hashes(kind: &str) -> Vec<String> {
    let g = lock();
    g.list
        .iter()
        .rev()
        .filter(|j| j.kind == kind && j.state == "unconfirmed")
        .filter_map(|j| j.tx_hash.clone())
        .collect()
}

/// `job {"id"}`: the job's record.
pub(crate) fn get(params: &Value) -> Result<Value, String> {
    let id = params["id"].as_u64().ok_or("job needs `id` (the number `claim` or `burn` answered)")?;
    let g = lock();
    let now = now_ms();
    g.list.iter().find(|j| j.id == id).map(|j| j.record(now)).ok_or_else(|| {
        format!(
            "no job {id} in this module: it keeps the jobs it ran since it started, in memory ({} kept, the last started {})",
            g.list.len(),
            g.next - 1
        )
    })
}

/// `jobs`: every job kept, oldest first, and the one running.
pub(crate) fn list() -> Value {
    let g = lock();
    let now = now_ms();
    json!({
        "jobs": g.list.iter().map(|j| j.record(now)).collect::<Vec<_>>(),
        "running": g.list.iter().find(|j| running(j.state)).map(|j| j.id),
        "kept_since_ms": g.since_ms,
        "now_ms": now,
        "kept": "in the module's memory, while the module runs",
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// The store is one per process: the tests that use it run one at a time.
    pub(crate) static SERIAL: Mutex<()> = Mutex::new(());

    pub(crate) fn serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn wait_done(id: u64) -> Value {
        let t0 = Instant::now();
        loop {
            let r = get(&json!({ "id": id })).expect("the job is kept");
            if r["running"] == false {
                return r;
            }
            assert!(t0.elapsed() < Duration::from_secs(10), "job {id} did not end: {r}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn unknown_id_and_missing_id() {
        let _s = serial();
        let e = get(&json!({ "id": 987_654 })).unwrap_err();
        assert!(e.starts_with("no job 987654 in this module"), "{e}");
        assert!(get(&json!({})).unwrap_err().contains("needs `id`"));
    }

    #[test]
    fn a_job_answers_at_once_runs_in_its_thread_and_settles() {
        let _s = serial();
        let (go, gate) = mpsc::channel::<()>();
        let t0 = Instant::now();
        let r = start("claim", &json!({ "leaf_index": 0, "private": true }), move |p| {
            gate.recv().ok();
            phase("proving", json!({ "by": "the test" }));
            phase("sent", json!({ "tx_hash": "0xabc" }));
            Ok(json!({ "settled": true, "private": true, "leaf": p["leaf_index"], "tx_hash": "0xabc", "block": 7 }))
        })
        .unwrap();
        assert!(t0.elapsed() < Duration::from_millis(500), "answered at once");
        assert_eq!(r["started"], true);
        let id = r["job"]["id"].as_u64().unwrap();
        assert_eq!(r["job"]["state"], "queued");
        assert_eq!(get(&json!({ "id": id })).unwrap()["running"], true);
        go.send(()).unwrap();
        let done = wait_done(id);
        assert_eq!(done["state"], "settled");
        assert_eq!(done["tx_hash"], "0xabc");
        assert_eq!(done["result"]["block"], 7);
        assert_eq!(done["result"]["leaf"], 0);
        let states: Vec<&str> = done["phases"].as_array().unwrap().iter().map(|p| p["state"].as_str().unwrap()).collect();
        assert_eq!(states, ["queued", "proving", "sent", "settled"]);
        assert!(done["proving_ms"].is_u64() && done["finished_ms"].is_u64());
        // A view opened again finds it in the listing.
        let l = list();
        assert!(l["jobs"].as_array().unwrap().iter().any(|j| j["id"] == id && j["state"] == "settled"));
        assert!(l["running"].is_null());
    }

    #[test]
    fn two_starts_of_one_step_give_one_job_and_another_step_is_refused() {
        let _s = serial();
        let (go, gate) = mpsc::channel::<()>();
        let first = start("burn", &json!({ "amount": "1" }), move |_| {
            gate.recv().ok();
            Ok(json!({ "settled": true }))
        })
        .unwrap();
        let id = first["job"]["id"].as_u64().unwrap();
        let again = start("burn", &json!({ "amount": "1" }), |_| panic!("a second burn must not start")).unwrap();
        assert_eq!(again["started"], false);
        assert_eq!(again["job"]["id"].as_u64(), Some(id));
        let other = start("claim", &json!({ "leaf_index": 0 }), |_| panic!("no claim while a burn runs")).unwrap_err();
        assert!(other.starts_with(&format!("refused: job {id} (the private burn)")), "{other}");
        assert_eq!(active().unwrap()["id"].as_u64(), Some(id));
        assert!(busy("scan", &active().unwrap()).starts_with(&format!("busy: job {id} (the private burn)")));
        go.send(()).unwrap();
        assert_eq!(wait_done(id)["state"], "settled");
        assert!(active().is_none());
    }

    #[test]
    fn a_failed_job_keeps_its_error_and_a_panic_fails_it() {
        let _s = serial();
        let r = start("claim", &json!({ "leaf_index": 9 }), |_| Err("not a deposit of this wallet".into())).unwrap();
        let done = wait_done(r["job"]["id"].as_u64().unwrap());
        assert_eq!(done["state"], "failed");
        assert_eq!(done["error"], "not a deposit of this wallet");
        assert!(done["result"].is_null());
        let r = start("claim", &json!({}), |_| panic!("boom")).unwrap();
        let done = wait_done(r["job"]["id"].as_u64().unwrap());
        assert_eq!(done["state"], "failed");
        assert!(done["error"].as_str().unwrap().contains("panicked"));
        // A reply that did not settle (reverted) is kept, and the job failed.
        let r = start("burn", &json!({}), |_| Ok(json!({ "settled": false, "what": "reverted in block 9" }))).unwrap();
        let done = wait_done(r["job"]["id"].as_u64().unwrap());
        assert_eq!(done["state"], "failed");
        assert_eq!(done["result"]["what"], "reverted in block 9");
        assert!(done["error"].as_str().unwrap().contains("reverted in block 9"));
    }

    /// A transaction broadcast and not yet seen mined is not a failure: the job ends `unconfirmed` (no "not deposited"
    /// for the view to show), its hash is kept, and the session refuses another deposit while it is unmined.
    #[test]
    fn a_send_not_yet_mined_ends_unconfirmed_not_failed() {
        let _s = serial();
        let h = format!("0x{}", "ab".repeat(32));
        let reply = json!({ "settled": false, "pending": true, "tx_hash": h, "what": "broadcast, not yet mined: transaction ..." });
        let r = start("deposit", &json!({}), move |_| Ok(reply)).unwrap();
        let done = wait_done(r["job"]["id"].as_u64().unwrap());
        assert_eq!(done["state"], "unconfirmed");
        assert_eq!(done["running"], false, "over: another step may start");
        assert!(done["error"].is_null(), "not a failure: {done}");
        assert_eq!(done["tx_hash"], json!(h));
        assert_eq!(unconfirmed_hashes("deposit").first(), Some(&h));
        assert!(unconfirmed_hashes("release").iter().all(|x| x != &h));
        assert!(active().is_none());
    }

    #[test]
    fn wait_runs_the_step_in_the_call_and_answers_as_the_call_did() {
        let _s = serial();
        let r = start("claim", &json!({ "leaf_index": 0, "wait": true }), |_| Ok(json!({ "settled": true, "block": 3 }))).unwrap();
        assert_eq!(r["block"], 3);
        let id = r["job"].as_u64().unwrap();
        assert_eq!(get(&json!({ "id": id })).unwrap()["run_by"], "the call itself (wait: true)");
        let e = start("burn", &json!({ "wait": true }), |_| Err("refused by the module".into())).unwrap_err();
        assert_eq!(e, "refused by the module");
    }
}
