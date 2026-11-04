//! Deterministic localhost protocol fixtures, not inference-provider tests.
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
use zeus::{
    agent::{self, ToolHost},
    model::{ChatClient, Protocol},
};

const WATCHDOG: Duration = Duration::from_secs(5);

struct Request {
    stream: TcpStream,
    body: Value,
}
impl Request {
    fn task(&self) -> usize {
        self.body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    }
    fn reply(mut self, message: Value) {
        let body = json!({"choices":[{"message":message}]}).to_string();
        write!(self.stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    }
    fn final_text(self) {
        let task = self.task();
        self.reply(json!({"role":"assistant","content":format!("answer-{task}")}));
    }
}

struct Fixture {
    client: ChatClient,
    address: std::net::SocketAddr,
    requests: mpsc::Receiver<Request>,
    stopped: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = ChatClient::with_protocol_timeout(
            &format!("http://{address}/"),
            "fixture",
            None,
            Protocol::ChatCompletions,
            WATCHDOG.as_secs(),
        )
        .unwrap();
        let (tx, requests) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let worker = thread::spawn(move || loop {
            let (stream, _) = listener.accept().unwrap();
            if stop.load(Ordering::SeqCst) {
                break;
            }
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(WATCHDOG)).unwrap();
            stream.set_write_timeout(Some(WATCHDOG)).unwrap();
            let mut reader = BufReader::new(stream);
            let mut length = None;
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((key, value)) = line.split_once(':') {
                    if key.eq_ignore_ascii_case("content-length") {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
            }
            let mut body = vec![0; length.unwrap()];
            reader.read_exact(&mut body).unwrap();
            if tx
                .send(Request {
                    stream: reader.into_inner(),
                    body: serde_json::from_slice(&body).unwrap(),
                })
                .is_err()
            {
                break;
            }
        });
        Self {
            client,
            address,
            requests,
            stopped,
            worker: Some(worker),
        }
    }
    fn next(&self) -> Request {
        self.requests
            .recv_timeout(WATCHDOG)
            .expect("expected concurrent request before releasing any response")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn limits_are_concurrent_and_results_keep_task_order() {
    for limit in 1..=3 {
        check_order_and_concurrency(limit, false);
    }
    check_order_and_concurrency(3, true);
}

#[test]
fn fast_worker_starts_queued_tasks_before_slow_sibling_finishes() {
    let fixture = Fixture::new();
    let tasks: Vec<_> = (0..5).map(|i| i.to_string()).collect();
    thread::scope(|scope| {
        let client = &fixture.client;
        let run = scope.spawn(move || {
            agent::run_parallel_with_limit(client, &NoTools, "coding worker", &tasks, 1, 2)
        });
        let mut first = vec![fixture.next(), fixture.next()];
        first.sort_by_key(Request::task);
        assert_eq!(first.iter().map(Request::task).collect::<Vec<_>>(), [0, 1]);
        first.pop().unwrap().final_text();
        let slow = first.pop().unwrap();
        // Task zero stays blocked for the entire queue drain. A batch barrier
        // cannot produce task two here; no throughput/timing guess is involved.
        for index in 2..5 {
            let request = fixture.next();
            assert_eq!(request.task(), index);
            assert!(
                fixture.requests.try_recv().is_err(),
                "worker bound exceeded"
            );
            request.final_text();
        }
        slow.final_text();
        let results = run.join().unwrap().unwrap();
        assert_eq!(
            results.iter().map(|r| r.text.clone()).collect::<Vec<_>>(),
            (0..5).map(|i| format!("answer-{i}")).collect::<Vec<_>>()
        );
    });
}

fn check_order_and_concurrency(limit: usize, legacy_api: bool) {
    let fixture = Fixture::new();
    let tasks: Vec<_> = (0..7).map(|i| i.to_string()).collect();
    thread::scope(|scope| {
        let client = &fixture.client;
        let tasks_ref = &tasks;
        let run = scope.spawn(move || {
            if legacy_api {
                agent::run_parallel(client, &NoTools, "read-only analyst", tasks_ref, 1)
            } else {
                agent::run_parallel_with_limit(
                    client,
                    &NoTools,
                    "read-only analyst",
                    tasks_ref,
                    1,
                    limit,
                )
            }
        });
        for batch in (0..tasks.len()).collect::<Vec<_>>().chunks(limit) {
            // A serial implementation deadlocks here: no response is released
            // until every worker in the configured batch has made a request.
            let mut pending: Vec<_> = batch.iter().map(|_| fixture.next()).collect();
            pending.sort_by_key(Request::task);
            assert_eq!(pending.iter().map(Request::task).collect::<Vec<_>>(), batch);
            for request in pending.into_iter().rev() {
                assert_eq!(request.body["messages"][0]["content"], "read-only analyst");
                request.final_text();
            }
        }
        let results = run.join().unwrap().unwrap();
        for (index, result) in results.iter().enumerate() {
            assert_eq!(result.text, format!("answer-{index}"));
            assert_eq!(result.messages[1]["content"], tasks[index]);
            assert_eq!(result.tool_calls, 0);
        }
        assert_eq!(results.len(), tasks.len());
        assert!(fixture.requests.try_recv().is_err());
    });
}

// Host calls expose deterministic lifecycle gates without invoking any external
// tool. Each started call remains in flight until the test explicitly releases it.
struct GatedHost {
    started: AtomicUsize,
    entered: mpsc::Sender<usize>,
    finished: mpsc::Sender<usize>,
    gates: Vec<Mutex<mpsc::Receiver<bool>>>,
}
impl ToolHost for GatedHost {
    fn schemas(&self) -> Vec<Value> {
        self.started.fetch_add(1, Ordering::SeqCst);
        vec![
            json!({"type":"function","function":{"name":"inspect","parameters":{"type":"object"}}}),
        ]
    }
    fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        assert_eq!(name, "inspect");
        let index = args["task"].as_u64().unwrap() as usize;
        self.entered.send(index).unwrap();
        let panic = self.gates[index]
            .lock()
            .unwrap()
            .recv_timeout(WATCHDOG)
            .unwrap();
        self.finished.send(index).unwrap();
        if panic {
            panic!("fixture worker panic");
        }
        Err(format!("fixture failure {index}"))
    }
}

#[test]
fn failures_and_panics_join_siblings_and_never_start_queued_tasks() {
    for (panic, order) in [(false, [0, 2, 1]), (true, [0, 2, 1]), (false, [2, 1, 0])] {
        let fixture = Fixture::new();
        let tasks: Vec<_> = (0..6).map(|i| i.to_string()).collect();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let (gates_tx, gates_rx): (Vec<_>, Vec<_>) = (0..3).map(|_| mpsc::channel()).unzip();
        let host = GatedHost {
            started: AtomicUsize::new(0),
            entered: entered_tx,
            finished: finished_tx,
            gates: gates_rx.into_iter().map(Mutex::new).collect(),
        };
        thread::scope(|scope| {
            let client = &fixture.client;
            let host = &host;
            let (done_tx, done_rx) = mpsc::channel();
            scope.spawn(move || {
                let result = agent::run_parallel_with_limit(client, host, "analyst", &tasks, 1, 3);
                done_tx.send(result).unwrap();
            });
            for _ in 0..3 {
                let request = fixture.next();
                let index = request.task();
                assert!(index < 3, "queued task started");
                request.reply(json!({"role":"assistant","tool_calls":[{"id":"inspect_1","type":"function","function":{"name":"inspect","arguments":json!({"task":index}).to_string()}}]}));
            }
            let mut entered: Vec<_> = (0..3)
                .map(|_| entered_rx.recv_timeout(WATCHDOG).unwrap())
                .collect();
            entered.sort();
            assert_eq!(entered, [0, 1, 2]);
            // Exercise both a failure immediately seen by the first join and
            // a later-index failure while an earlier worker remains blocked.
            gates_tx[order[0]].send(panic).unwrap();
            assert_eq!(finished_rx.recv_timeout(WATCHDOG).unwrap(), order[0]);
            assert!(
                matches!(
                    done_rx.recv_timeout(Duration::from_millis(100)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ),
                "returned without joining blocked siblings"
            );
            // Both sibling slots are still occupied after the known failure.
            assert!(
                fixture.requests.try_recv().is_err(),
                "started queued work after failure"
            );
            for &index in &order[1..] {
                gates_tx[index].send(false).unwrap();
                assert_eq!(finished_rx.recv_timeout(WATCHDOG).unwrap(), index);
            }
            let result = done_rx.recv_timeout(WATCHDOG).unwrap();
            assert_eq!(
                result.unwrap_err(),
                if panic {
                    "agent worker panicked"
                } else {
                    "fixture failure 0"
                }
            );
            assert!(
                fixture.requests.try_recv().is_err(),
                "queued batch was not cancelled"
            );
            assert!(entered_rx.try_recv().is_err());
            assert_eq!(
                host.started.load(Ordering::SeqCst),
                3,
                "queued workers were started"
            );
        });
    }
}

struct NoTools;

#[test]
fn successful_sibling_does_not_refill_queue_after_failure() {
    let fixture = Fixture::new();
    let tasks: Vec<_> = (0..6).map(|i| i.to_string()).collect();
    thread::scope(|scope| {
        let client = &fixture.client;
        let (done_tx, done_rx) = mpsc::channel();
        scope.spawn(move || {
            done_tx
                .send(agent::run_parallel_with_limit(
                    client, &NoTools, "analyst", &tasks, 1, 3,
                ))
                .unwrap();
        });
        let mut pending: Vec<_> = (0..3).map(|_| fixture.next()).collect();
        pending.sort_by_key(Request::task);
        // Unknown tool names produce a recoverable denial; with one turn this
        // task then fails its budget. Siblings still have withheld responses.
        pending
            .remove(0)
            .reply(json!({"role":"assistant","tool_calls":[{
                "id":"denied", "type":"function", "function":{
                    "name":"unadvertised", "arguments":"{}"
                }
            }]}));
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        for request in pending {
            request.final_text();
        }
        let result = done_rx.recv_timeout(WATCHDOG).unwrap();
        assert_eq!(result.unwrap_err(), "agent exhausted its turn limit");
        assert!(
            fixture.requests.try_recv().is_err(),
            "successful sibling admitted queued work after failure"
        );
    });
}

impl ToolHost for NoTools {
    fn schemas(&self) -> Vec<Value> {
        vec![]
    }
    fn call(&self, _: &str, _: Value) -> Result<Value, String> {
        panic!("no capability advertised")
    }
}

#[test]
fn worker_limit_is_validated_even_for_empty_tasks() {
    let client = ChatClient::new("http://127.0.0.1:1/", "fixture", None).unwrap();
    for limit in [0, 4, usize::MAX] {
        assert_eq!(
            agent::run_parallel_with_limit(&client, &NoTools, "s", &[], 1, limit).unwrap_err(),
            "worker limit must be between 1 and 3"
        );
    }
    for limit in 1..=3 {
        assert!(
            agent::run_parallel_with_limit(&client, &NoTools, "s", &[], 1, limit)
                .unwrap()
                .is_empty()
        );
    }
}
