use crate::error::{SimpleError, SimpleResult};
use chrono::prelude::*;
use cookie::{Cookie, CookieBuilder};
use prettytable::{Cell, Row, Table};
use reqwest::{header, Client, Url};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug)]
pub struct Scoreboard {
    user_map: BTreeMap<u64, UserRecord>,
    problem_set: BTreeSet<u32>,
    latest_submission: Option<DateTime<Local>>,
}

impl Scoreboard {
    pub fn new() -> Self {
        Self {
            user_map: BTreeMap::new(),
            problem_set: BTreeSet::new(),
            latest_submission: None,
        }
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
        let count = json["msg"]["count"]
            .as_u64()
            .ok_or("msg::count not found")? as usize;

        let mut submission_list: Vec<Value> = json["msg"]["submissions"]
            .as_array()
            .ok_or("msg::submission not found")?.clone();
        submission_list.reverse();
        for v in submission_list {
            let user_id = v["user_id"]
                .as_u64()
                .ok_or("submission::user_id not found")?;
            let user_record: &mut UserRecord = board.user_map.entry(user_id).or_default();
            if user_record.name.is_empty() {
                let mut respond = client
                    .get(format!("https://api.oj.nctu.me/users/{}/", user_id).as_str())
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

            match v["verdict_id"]
                .as_u64()
                .ok_or("msg::verdict_id not found")?
            {
                4..=9 => {
                    if user_record.problem(*prob).status != SolveStatus::Accepted {
                        user_record.problem(*prob).status = SolveStatus::WrongAnswer;
                        user_record.problem(*prob).wa_count += 1;
                    }
                }
                10 => {
                    user_record.problem(*prob).status = SolveStatus::Accepted;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

impl Default for Scoreboard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
struct UserRecord {
    id: u64,
    name: String,
    problems: BTreeMap<u32, ProblemCell>,
}

impl UserRecord {
    fn new() -> Self {
        Self::default()
    }

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

#[derive(Debug, Clone, Copy, Default)]
struct ProblemCell {
    wa_count: usize,
    status: SolveStatus,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SolveStatus {
    None,
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
