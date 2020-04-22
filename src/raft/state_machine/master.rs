use self::configs::{Configures, RaftMember, CONFIG_SM_ID};
use super::super::*;
use super::*;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ExecError {
    SmNotFound,
    FnNotFound,
    ServersUnreachable,
    CannotConstructClient,
    NotCommitted,
    Unknown,
    TooManyRetry,
}

pub enum RegisterResult {
    OK,
    EXISTED,
    RESERVED,
}

pub type ExecOk = Vec<u8>;
pub type ExecResult = Result<ExecOk, ExecError>;
pub type SubStateMachine = Box<StateMachineCtl>;
pub type SnapshotDataItem = (u64, Vec<u8>);
pub type SnapshotDataItems = Vec<SnapshotDataItem>;

raft_state_machine! {}

pub struct MasterStateMachine {
    subs: HashMap<u64, SubStateMachine>,
    snapshots: HashMap<u64, Vec<u8>>,
    pub configs: Configures,
}

impl StateMachineCmds for MasterStateMachine {}

impl StateMachineCtl for MasterStateMachine {
    raft_sm_complete!();
    fn id(&self) -> u64 {
        0
    }
    fn snapshot(&self) -> Option<Vec<u8>> {
        let mut sms: SnapshotDataItems = Vec::with_capacity(self.subs.len());
        for (sm_id, smc) in self.subs.iter() {
            let sub_snapshot = smc.snapshot();
            if let Some(snapshot) = sub_snapshot {
                sms.push((*sm_id, snapshot));
            }
        }
        sms.push((self.configs.id(), self.configs.snapshot().unwrap()));
        let data = crate::utils::serde::serialize(&sms);
        Some(data)
    }
    fn recover(&mut self, data: Vec<u8>) -> BoxFuture<()> {
        let mut sms: SnapshotDataItems = crate::utils::serde::deserialize(data.as_slice()).unwrap();
        for (sm_id, snapshot) in sms {
            self.snapshots.insert(sm_id, snapshot);
        }
        future::ready(()).boxed()
    }
}

fn parse_output(r: Option<Vec<u8>>) -> ExecResult {
    if let Some(d) = r {
        Ok(d)
    } else {
        Err(ExecError::FnNotFound)
    }
}

impl MasterStateMachine {
    pub fn new(service_id: u64) -> MasterStateMachine {
        let mut msm = MasterStateMachine {
            subs: HashMap::new(),
            snapshots: HashMap::new(),
            configs: Configures::new(service_id),
        };
        msm
    }

    pub fn register(&mut self, mut smc: SubStateMachine) -> RegisterResult {
        let id = smc.id();
        if id < 2 {
            return RegisterResult::RESERVED;
        }
        if self.subs.contains_key(&id) {
            return RegisterResult::EXISTED;
        };
        if let Some(snapshot) = self.snapshots.remove(&id) {
            smc.recover(snapshot);
        }
        self.subs.insert(id, smc);
        RegisterResult::OK
    }

    pub fn members(&self) -> &HashMap<u64, RaftMember> {
        &self.configs.members
    }

    pub async fn commit_cmd(&mut self, entry: &LogEntry) -> ExecResult {
        match entry.sm_id {
            CONFIG_SM_ID => {
                parse_output(self.configs.fn_dispatch_cmd(entry.fn_id, &entry.data).await)
            }
            _ => {
                if let Some(sm) = self.subs.get_mut(&entry.sm_id) {
                    parse_output(sm.as_mut().fn_dispatch_cmd(entry.fn_id, &entry.data).await)
                } else {
                    debug!("Cannot find state machine {} for command, we have {:?}",
                           entry.id, self.subs.keys().collect::<Vec<_>>());
                    Err(ExecError::SmNotFound)
                }
            }
        }
    }
    pub async fn exec_qry(&self, entry: &LogEntry) -> ExecResult {
        match entry.sm_id {
            CONFIG_SM_ID => {
                parse_output(self.configs.fn_dispatch_qry(entry.fn_id, &entry.data).await)
            }
            _ => {
                if let Some(sm) = self.subs.get(&entry.sm_id) {
                    parse_output(sm.fn_dispatch_qry(entry.fn_id, &entry.data).await)
                } else {
                    debug!("Cannot find state machine {} for query, we have {:?}",
                           entry.id, self.subs.keys().collect::<Vec<_>>());
                    Err(ExecError::SmNotFound)
                }
            }
        }
    }
    pub fn clear_subs(&mut self) {
        self.subs.clear()
    }
    pub fn has_sub(&self, id: &u64) -> bool {
        self.subs.contains_key(&id)
    }
}

impl Error for ExecError {}
impl Display for ExecError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
