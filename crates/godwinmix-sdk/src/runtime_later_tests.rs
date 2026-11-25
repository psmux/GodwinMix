//! A service that answers a slow tool later: the calls behind it are
//! answered first, and a dropped reply still answers.

use super::*;
use crate::plugin::{Service, ServiceHandler};
use crate::reply::Reply;
use crate::wire::{Configure, InitializeResult, ToolResult};
use std::time::{Duration, Instant};

struct Slow;

impl Service for Slow {
    fn initialize(&mut self, _: &Ready, _: Reporter) -> Result<InitializeResult, RpcError> {
        Ok(InitializeResult::default())
    }
    fn configure(&mut self, _: Value) -> Result<Configure, RpcError> {
        Ok(Configure::applied())
    }
    fn tool_call_later(&mut self, name: &str, _: Value, reply: Reply) {
        let answer = ToolResult { content: serde_json::json!([{"type": "text", "text": name}]), structured_content: None, is_error: None };
        match name {
            "slow" => {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    reply.tool(Ok(answer));
                });
            }
            "lost" => drop(reply),
            _ => reply.tool(Ok(answer)),
        }
    }
}

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Sink {
    fn ids(&self) -> Vec<Value> {
        let text = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
        text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v.get("id").is_some()).collect()
    }
}

fn call(jobs: &Sender<Job>, id: i64, name: &str) {
    let params = serde_json::json!({"name": name, "arguments": {}});
    jobs.send(Job::Call { id: Some(Id::Num(id)), method: "tool.call".into(), params }).unwrap();
}

#[test]
fn a_slow_tool_answered_later_does_not_hold_up_the_next_call() {
    let sink = Sink::default();
    let machine = Arc::new(Mutex::new(Machine::new()));
    machine.lock().unwrap().initialized();
    let (jobs, worker) = spawn_worker(ServiceHandler(Slow), Writer::new(Box::new(sink.clone())), Arc::new(Mutex::new(Health::ok())), machine);
    let start = Instant::now();
    call(&jobs, 1, "slow");
    call(&jobs, 2, "fast");
    call(&jobs, 3, "lost");
    while sink.ids().len() < 2 {
        assert!(start.elapsed() < Duration::from_secs(1), "the fast calls were not answered");
        std::thread::sleep(Duration::from_millis(2));
    }
    let first = sink.ids();
    assert_eq!(first[0]["id"], 2, "the fast call is answered before the slow one: {first:?}");
    assert!(first[1]["error"]["message"].as_str().unwrap().contains("without answering"), "{first:?}");
    assert!(start.elapsed() < Duration::from_millis(250));
    while sink.ids().len() < 3 {
        assert!(start.elapsed() < Duration::from_secs(2), "the slow call never answered");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(sink.ids()[2]["result"]["content"][0]["text"], "slow");
    drop(jobs);
    worker.join().unwrap();
}
