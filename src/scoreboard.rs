use crate::error::SimpleResult;
use chrono::prelude::*;
use prettytable::{Row, Table};
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

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

    pub fn gen_table(&self) -> Table {
        let mut table = Table::new();
        let mut users: Vec<&UserRecord> = self.user_map.iter().map(|p| p.1).collect();
        users.sort_by(|&a, &b| {
            b.ac_count(&self.problem_set)
                .cmp(&a.ac_count(&self.problem_set))
        });

        let mut head_cells = Vec::new();
        head_cells.push(cell!(""));
        for prob in &self.problem_set {
            head_cells.push(cell!(c->prob));
        }
        table.add_row(Row::new(head_cells));

        for user in &users {
            let mut cells = Vec::new();
            cells.push(cell!(c->user.name));
            for prob in &self.problem_set {
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

        let mut footer_cells = Vec::new();
        footer_cells.push(cell!(c->"Updated At"));
        for prob in &self.problem_set {
            match self.problem_cache.get(prob) {
                Some(t) => footer_cells.push(cell!(c->format!("{}\n{}", t.format("%Y-%m-%d"), t.format("%H:%M:%S")))),
                None => footer_cells.push(cell!("")),
            }
        }
        table.add_row(Row::new(footer_cells));

        table
    }
}

pub fn sync(board: &mut Scoreboard, token: &str) -> SimpleResult<()> {
    let client = Client::new();
    let cookie = format!("token={}", token);
    for prob in &board.problem_set {
        let request = client
            .get("https://api.oj.nctu.me/submissions/")
            .header(
                header::COOKIE,
                header::HeaderValue::from_bytes(cookie.as_bytes()).unwrap(),
            )
            .query(&[("group_id", &11.to_string())])
            .query(&[("count", &100000.to_string())])
            .query(&[("page", &1.to_string())])
            .query(&[("problem_id", &prob.to_string())])
            .build()?;
        let mut respond = client.execute(request)?;
        let json: Value = serde_json::from_str(&respond.text()?)?;
        let _count = json["msg"]["count"]
            .as_u64()
            .ok_or("msg::count not found")? as usize;

        let mut submission_list: Vec<Submission> = json["msg"]["submissions"]
            .as_array()
            .ok_or("msg::submissions not found")?
            .iter()
            .map(|json| Submission::from_json(json))
            .collect::<SimpleResult<Vec<Submission>>>()?;
        submission_list.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));

        let mut time = match board.problem_cache.get(prob) {
            Some(t) => t.clone(),
            None => DateTime::<Local>::from(std::time::UNIX_EPOCH),
        };

        let start_from = match submission_list.binary_search_by(|sub| sub.updated_at.cmp(&time)) {
            Ok(p) => p + 1,
            Err(p) => p
        };

        for sub in &submission_list[start_from..] {
            let user_record: &mut UserRecord = board.user_map.entry(sub.user_id).or_default();
            if user_record.name.is_empty() {
                let mut respond = client
                    .get(format!("https://api.oj.nctu.me/users/{}/", sub.user_id).as_str())
                    .header(
                        header::COOKIE,
                        header::HeaderValue::from_bytes(cookie.as_bytes()).unwrap(),
                    )
                    .send()?;
                let user_json: Value = serde_json::from_str(&respond.text()?)?;
                user_record.name = user_json["msg"]["name"]
                    .as_str()
                    .ok_or("user::msg::name not found")?
                    .to_owned();
            }

            match sub.verdict_id {
                4..=9 => {
                    if user_record.problem(*prob).status != SolveStatus::Accepted {
                        user_record.problem(*prob).status = SolveStatus::WrongAnswer;
                        user_record.problem(*prob).wa_count += 1;
                    }
                    if sub.updated_at > time {
                        time = sub.updated_at;
                    }
                }
                10 => {
                    user_record.problem(*prob).status = SolveStatus::Accepted;
                    if sub.updated_at > time {
                        time = sub.updated_at;
                    }
                }
                _ => {}
            }
        }

        board
            .problem_cache
            .entry(*prob)
            .and_modify(|t| {
                if time > *t {
                    t.clone_from(&time);
                }
            })
            .or_insert(time);
    }
    Ok(())
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
