use crate::error::SimpleResult;
use chrono::prelude::*;
use futures::future::Future;
use futures::stream::Stream;
use prettytable::{Row, Table};
use reqwest::header;
use reqwest::header::HeaderMap;
use reqwest::r#async::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio_timer::clock::Clock;

#[derive(Debug, Serialize, Deserialize)]
pub struct Scoreboard {
    user_map: BTreeMap<u64, UserRecord>,
    problem_set: BTreeSet<u32>,
    problem_cache: BTreeMap<u32, DateTime<Local>>,
}

impl Scoreboard {
    pub fn new() -> Self {
        Self {
            user_map: BTreeMap::new(),
            problem_set: BTreeSet::new(),
            problem_cache: BTreeMap::new(),
        }
    }

    pub fn load_cache<P: AsRef<Path>>(path: P) -> SimpleResult<Self> {
        let f = fs::OpenOptions::new().read(true).open(path)?;
        Ok(bincode::deserialize_from(f)?)
    }

    pub fn save_cache<P: AsRef<Path>>(&self, path: P) -> SimpleResult<()> {
        let f = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)?;
        bincode::serialize_into(f, self)?;
        Ok(())
    }

    pub fn add_problem(&mut self, problem_id: u32) {
        self.problem_set.insert(problem_id);
    }

    pub fn remove_problem(&mut self, problem_id: u32) {
        self.problem_set.remove(&problem_id);
    }

    pub fn gen_table(&self, problems: &[u32]) -> Table {
        let mut table = Table::new();
        let mut users: Vec<&UserRecord> = self.user_map.iter().map(|p| p.1).collect();
        users.sort_by(|&a, &b| {
            b.ac_count(&self.problem_set)
                .cmp(&a.ac_count(&self.problem_set))
        });

        

        // Generate problems' ID
        let mut prob_cells = Vec::new();
        prob_cells.push(cell!(""));
        for prob in problems {
            prob_cells.push(cell!(c->prob));
        }
        table.add_row(Row::new(prob_cells.clone()));

        // Generate Update Time
        let mut update_cells = Vec::new();
        update_cells.push(cell!(c->"Updated At"));
        for prob in problems {
            match self.problem_cache.get(prob) {
                Some(t) => update_cells
                    .push(cell!(c->format!("{}\n{}", t.format("%Y-%m-%d"), t.format("%H:%M:%S")))),
                None => update_cells.push(cell!("")),
            }
        }
        table.add_row(Row::new(update_cells));

        // Generate User Solving Status
        for user in &users {
            let mut cells = Vec::new();
            cells.push(cell!(c->user.name));
            for prob in problems {
                let p = &user.problems.get(&prob).map(|x| *x).unwrap_or_default();
                let c = match p.status {
                    SolveStatus::Accepted => {
                        cell!(Fgc->format!("{} / {}", p.status, p.wa_count + 1))
                    }
                    SolveStatus::WrongAnswer => {
                        cell!(Frc->format!("{} / {}", p.status, p.wa_count))
                    }
                    SolveStatus::None => cell!(FDc->format!("{}", p.status)),
                };
                cells.push(c);
            }
            table.add_row(Row::new(cells));
        }

        // Also generate one at footer
        table.add_row(Row::new(prob_cells.clone()));

        table
    }

    pub fn sync(&mut self, token: &str) -> SimpleResult<()> {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, format!("token={}", token).parse().unwrap());

        let client = Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        let client_cloned = client.clone();

        let mut runtime = tokio::runtime::Builder::new().clock(Clock::new()).build()?;

        let problem_list: Vec<u32> = self.problem_set.iter().cloned().collect();
        let problem_futures = futures::stream::iter_ok(problem_list)
            .map(move |pid| {
                client
                    .get("https://api.oj.nctu.me/submissions/")
                    .query(&[("group_id", &11.to_string())])
                    .query(&[("count", &100000.to_string())])
                    .query(&[("page", &1.to_string())])
                    .query(&[("problem_id", pid.to_string())])
                    .send()
                    .and_then(|mut res| res.json())
                    .then(
                        move |json: Result<Value, reqwest::Error>| -> SimpleResult<(u32, Vec<Submission>)> {
                            let json = json?;
                            let mut submission_list: Vec<Submission> = json["msg"]["submissions"]
                                .as_array()
                                .ok_or("msg::submissions not found")?
                                .iter()
                                .map(|json| Submission::from_json(json))
                                .collect::<SimpleResult<Vec<Submission>>>()?;
                            submission_list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                            Ok((pid, submission_list))
                        },
                    )
            })
            .buffer_unordered(10)
            .collect();
        let problems = runtime.block_on(problem_futures)?;
        for (pid, submissions) in problems {
            let mut time = match self.problem_cache.get(&pid) {
                Some(t) => t.clone(),
                None => DateTime::<Local>::from(std::time::UNIX_EPOCH),
            };

            let start_from = match submissions.binary_search_by(|sub| sub.created_at.cmp(&time)) {
                Ok(p) => p + 1,
                Err(p) => p,
            };

            for sub in &submissions[start_from..] {
                let user_record: &mut UserRecord = self.user_map.entry(sub.user_id).or_default();

                match sub.verdict_id {
                    4..=9 => {
                        if user_record.problem(pid).status != SolveStatus::Accepted {
                            user_record.problem(pid).status = SolveStatus::WrongAnswer;
                            user_record.problem(pid).wa_count += 1;
                        }
                        if sub.created_at > time {
                            time = sub.created_at;
                        }
                    }
                    10 => {
                        user_record.problem(pid).status = SolveStatus::Accepted;
                        if sub.created_at > time {
                            time = sub.created_at;
                        }
                    }
                    _ => {}
                }
            }

            self
                .problem_cache
                .entry(pid)
                .and_modify(|t| {
                    if time > *t {
                        t.clone_from(&time);
                    }
                })
                .or_insert(time);
        }

        let client = client_cloned;

        // Fetch all user's name
        let need_update: Vec<u64> = self
            .user_map
            .iter()
            .filter_map(|(&uid, user)| {
                if user.name.is_empty() {
                    Some(uid)
                } else {
                    None
                }
            })
            .collect();
        let username_futures = futures::stream::iter_ok(need_update)
            .map(move |uid| {
                let url = format!("https://api.oj.nctu.me/users/{}/", uid);
                debug!("-> fetching {} ...", url);
                client
                    .get(url.as_str())
                    .send()
                    .and_then(|mut res| res.json())
                    .then(
                        move |user_json: Result<Value, reqwest::Error>| -> SimpleResult<(u64, String)> {
                            debug!("<- {} data: {:?}", uid, user_json);
                            let name = user_json?["msg"]["name"]
                                .as_str()
                                .ok_or("user::msg::name not found")?
                                .to_owned();
                            Ok((uid, name))
                        },
                    )
            })
            .buffer_unordered(10)
            .collect();

        let mut runtime = tokio::runtime::Builder::new().clock(Clock::new()).build()?;
        let usernames = runtime.block_on(username_futures)?;
        for (uid, name) in usernames {
            self.user_map.entry(uid).and_modify(|user| {
                user.name = name;
            });
        }
        runtime.shutdown_on_idle().wait().unwrap();
        info!("Sync completed");

        Ok(())
    }
}


impl Default for Scoreboard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UserRecord {
    id: u64,
    name: String,
    problems: BTreeMap<u32, ProblemCell>,
}

impl UserRecord {
    fn ac_count(&self, prob_set: &BTreeSet<u32>) -> usize {
        let mut count = 0;
        for prob in prob_set {
            if let Some(cell) = self.problems.get(prob) {
                if cell.status == SolveStatus::Accepted {
                    count += 1;
                }
            }
        }
        count
    }

    fn problem(&mut self, prob_id: u32) -> &mut ProblemCell {
        self.problems.entry(prob_id).or_default()
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct ProblemCell {
    wa_count: usize,
    status: SolveStatus,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum SolveStatus {
    None = 0,
    Accepted,
    WrongAnswer,
}

impl fmt::Display for SolveStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !f.alternate() {
            match self {
                SolveStatus::Accepted => write!(f, "AC"),
                SolveStatus::WrongAnswer => write!(f, "WA"),
                SolveStatus::None => write!(f, "NS"),
            }
        } else {
            match self {
                SolveStatus::Accepted => write!(f, "Accepted"),
                SolveStatus::WrongAnswer => write!(f, "Wrong Answer"),
                SolveStatus::None => write!(f, "None"),
            }
        }
    }
}

impl Default for SolveStatus {
    fn default() -> Self {
        SolveStatus::None
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Submission {
    memory_usage: Option<u64>,
    time_usage: Option<u64>,
    length: usize,
    verdict_id: u32,
    execute_id: u32,
    user_id: u64,
    problem_id: u32,
    #[serde(with = "simple_datetime")]
    created_at: DateTime<Local>,
    #[serde(with = "simple_datetime")]
    updated_at: DateTime<Local>,
    id: u64,
    score: Option<i32>,
}

impl Submission {
    fn from_json(json: &Value) -> SimpleResult<Self> {
        Ok(serde_json::from_value(json.clone())?)
    }
}

// This module is modified from serde's example
// See https://serde.rs/custom-date-format.html
mod simple_datetime {
    use chrono::{DateTime, Local, TimeZone};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &'static str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &DateTime<Local>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Local>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Local
            .datetime_from_str(&s, FORMAT)
            .map_err(serde::de::Error::custom)
    }
}
