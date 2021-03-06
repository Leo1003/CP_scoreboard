use crate::api::*;
use crate::error::*;
use chrono::prelude::*;
use futures::future::Future;
use prettytable::{format::Alignment, Cell, Row, Table};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Serialize, Deserialize)]
pub struct Scoreboard {
    user_map: Mutex<BTreeMap<u32, UserRecord>>,
    problem_set: Mutex<BTreeSet<u32>>,
    cache_time: RwLock<DateTime<Local>>,
}

impl Scoreboard {
    pub fn new() -> Self {
        Self {
            user_map: Mutex::new(BTreeMap::new()),
            problem_set: Mutex::new(BTreeSet::new()),
            cache_time: RwLock::new(DateTime::<Local>::from(std::time::UNIX_EPOCH)),
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

    pub fn gen_table(&self, problems: Option<&[u32]>) -> Table {
        let mut table = Table::new();
        let user_lock = self.user_map.lock().unwrap();
        let mut users: Vec<&UserRecord> = user_lock.iter().map(|p| p.1).collect();
        let problems_lock = self.problem_set.lock().unwrap();

        users.sort_by(|&a, &b| b.ac_count(&problems_lock).cmp(&a.ac_count(&problems_lock)));

        // Generate the actual problem list
        let prob_list: Cow<[u32]> = if let Some(problems) = problems {
            Cow::from(problems)
        } else {
            let set_list: Vec<u32> = problems_lock.iter().copied().collect();
            Cow::from(set_list)
        };
        debug!("{:?}", prob_list);

        // Generate problems' ID
        let mut prob_cells = Vec::new();
        prob_cells.push(cell!(""));
        for prob in prob_list.iter() {
            prob_cells.push(cell!(c->prob));
        }
        table.add_row(Row::new(prob_cells.clone()));

        // Generate Update Time
        let mut update_row = Vec::new();
        update_row.push(cell!(c->"Updated At"));

        let t = self.cache_time.read().unwrap();
        let mut update_cell = Cell::new_align(
            format!("{}\n{}", t.format("%Y-%m-%d"), t.format("%H:%M:%S")).as_str(),
            Alignment::CENTER,
        );
        update_cell.set_hspan(prob_list.len());
        update_row.push(update_cell);

        table.add_row(Row::new(update_row));

        // Generate User Solving Status
        for user in &users {
            let mut cells = Vec::new();
            let mut should_display = false;
            cells.push(cell!(c->user.name));
            for prob in prob_list.iter() {
                let p = &user.problems.get(&prob).copied().unwrap_or_default();
                // Make all 'NS' not display
                let c = match p.status {
                    SolveStatus::Accepted => {
                        should_display = true;
                        cell!(Fgc->format!("{} / {}", p.status, p.wa_count + 1))
                    }
                    SolveStatus::WrongAnswer => {
                        should_display = true;
                        cell!(Frc->format!("{} / {}", p.status, p.wa_count))
                    }
                    SolveStatus::None => cell!(FDc->format!("{}", p.status)),
                };
                cells.push(c);
            }
            if should_display {
                table.add_row(Row::new(cells));
            }
        }

        // Also generate one at footer
        table.add_row(Row::new(prob_cells.clone()));

        table
    }
}

pub fn sync(
    board: Arc<Scoreboard>,
    gid: u32,
    token: String,
) -> impl Future<Item = (), Error = SimpleError> + 'static {
    let board_arc = board.clone();
    futures::future::result(FojApi::new(token))
        .and_then(|foj| {
            foj.session()
                .map(|session| {
                    info!("Authentication Succuss!");
                    trace!("{:?}", session);
                    Arc::new(foj)
                })
                .map_err(|_| "Authentication Failed!".into())
        })
        .and_then(move |foj| {
            let foj_arc = foj.clone();
            fetch_group(board.clone(), foj_arc.clone(), gid).map(move |_| foj)
        })
        .and_then(move |foj| update_name(board_arc, foj))
}

fn fetch_group(
    board: Arc<Scoreboard>,
    foj: Arc<FojApi>,
    gid: u32,
) -> impl Future<Item = (), Error = SimpleError> {
    foj.get_submission_group(gid)
        .map(move |mut submissions| {
            submissions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            submissions
        })
        .and_then(move |submissions| save_submissions(board, submissions))
}

fn save_submissions(board: Arc<Scoreboard>, submissions: Vec<Submission>) -> SimpleResult<()> {
    let time_lock = board.cache_time.read().unwrap();
    let mut new_time = *time_lock;

    let start_from = match submissions.binary_search_by(|sub| sub.created_at.cmp(&time_lock)) {
        Ok(p) => p + 1,
        Err(p) => p,
    };

    let mut user_lock = board.user_map.lock().unwrap();
    let mut problems_lock = board.problem_set.lock().unwrap();

    for sub in &submissions[start_from..] {
        let user_record: &mut UserRecord = user_lock.entry(sub.user_id).or_default();
        let pid = sub.problem_id;

        if !problems_lock.contains(&pid) {
            problems_lock.insert(pid);
        }

        match sub.verdict_id as u32 {
            4..=9 => {
                if user_record.problem(pid).status != SolveStatus::Accepted {
                    user_record.problem(pid).status = SolveStatus::WrongAnswer;
                    user_record.problem(pid).wa_count += 1;
                }
                if sub.created_at > new_time {
                    new_time = sub.created_at;
                }
            }
            10 => {
                user_record.problem(pid).status = SolveStatus::Accepted;
                if sub.created_at > new_time {
                    new_time = sub.created_at;
                }
            }
            _ => {}
        }
    }

    drop(time_lock);
    let mut time_entry = board.cache_time.write().unwrap();
    if new_time > *time_entry {
        *time_entry = new_time;
    }
    Ok(())
}

fn update_name(
    board: Arc<Scoreboard>,
    foj: Arc<FojApi>,
) -> impl Future<Item = (), Error = SimpleError> {
    let name_update_list: Vec<u32> = board
        .user_map
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(&uid, user)| {
            if user.name.is_empty() {
                Some(uid)
            } else {
                None
            }
        })
        .collect();
    let futures_iter = name_update_list.into_iter().map(move |uid| {
        let board = board.clone();
        foj.get_user_name(uid)
            .map(move |name| (uid, name))
            .map(move |(uid, name)| {
                board
                    .user_map
                    .lock()
                    .unwrap()
                    .entry(uid)
                    .and_modify(|user| {
                        user.name = name;
                    });
            })
    });
    futures::future::join_all(futures_iter).map(|_| ())
}

impl Default for Scoreboard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UserRecord {
    id: u32,
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
