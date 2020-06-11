// Group membership manager regardless actual raft members

pub mod client;
pub mod member;
pub mod server;

use crate::membership::client::Member as ClientMember;
use bifrost_plugins::hash_ident;

pub static DEFAULT_SERVICE_ID: u64 = hash_ident!(BIFROST_MEMBERSHIP_SERVICE) as u64;

pub mod raft {
    use super::*;
    raft_state_machine! {
        def cmd hb_online_changed(online: Vec<u64>, offline: Vec<u64>);
        def cmd join(address: String) -> Option<u64>;
        def cmd leave(id: u64) -> bool;
        def cmd join_group(group_name: String, id: u64) -> bool;
        def cmd leave_group(group: u64, id: u64) -> bool;
        def cmd new_group(name: String) -> Result<u64, u64>;
        def cmd del_group(id: u64) -> bool;
        def qry group_leader(group: u64) -> Option<(Option<ClientMember>, u64)>;
        def qry group_members (group: u64, online_only: bool) -> Option<(Vec<ClientMember>, u64)>;
        def qry all_members (online_only: bool) -> (Vec<ClientMember>, u64);
        def sub on_group_member_offline(group: u64) -> (ClientMember, u64); //
        def sub on_any_member_offline() -> (ClientMember, u64); //
        def sub on_group_member_online(group: u64) -> (ClientMember, u64); //
        def sub on_any_member_online() -> (ClientMember, u64); //
        def sub on_group_member_joined(group: u64) -> (ClientMember, u64); //
        def sub on_any_member_joined() -> (ClientMember, u64); //
        def sub on_group_member_left(group: u64) -> (ClientMember, u64); //
        def sub on_any_member_left() -> (ClientMember, u64); //
        def sub on_group_leader_changed(group: u64) -> (Option<ClientMember>, Option<ClientMember>, u64);
    }
}

// The service only responsible for receiving heartbeat and
// Updating last updated time
// Expired update time will trigger timeout in the raft state machine
mod heartbeat_rpc {
    service! {
        rpc ping(id: u64);
    }
}

#[cfg(test)]
mod test {
    use crate::membership::client::ObserverClient;
    use crate::membership::member::MemberService;
    use crate::membership::server::Membership;
    use crate::raft::client::RaftClient;
    use crate::raft::{Options, RaftService, Storage, DEFAULT_SERVICE_ID};
    use crate::rpc::Server;
    use crate::utils::time::async_wait_secs;
    use futures::prelude::*;
    use std::sync::atomic::*;
    use std::sync::Arc;

    #[tokio::test(threaded_scheduler)]
    async fn primary() {
        env_logger::try_init().unwrap();
        let addr = String::from("127.0.0.1:2100");
        let raft_service = RaftService::new(Options {
            storage: Storage::default(),
            address: addr.clone(),
            service_id: DEFAULT_SERVICE_ID,
        });
        let server = Server::new(&addr);
        server
            .register_service(DEFAULT_SERVICE_ID, &raft_service)
            .await;
        Server::listen_and_resume(&server).await;
        RaftService::start(&raft_service).await;
        Membership::new(&server, &raft_service, true).await;
        raft_service.bootstrap().await;

        let group_1 = String::from("test_group_1");
        let group_2 = String::from("test_group_2");
        let group_3 = String::from("test_group_3");

        let wild_raft_client = RaftClient::new(&vec![addr.clone()], DEFAULT_SERVICE_ID)
            .await
            .unwrap();
        let client = ObserverClient::new(&wild_raft_client);

        RaftClient::prepare_subscription(&server).await;

        client.new_group(&group_1).await.unwrap().unwrap();
        client.new_group(&group_2).await.unwrap().unwrap();
        client.new_group(&group_3).await.unwrap().unwrap();

        let any_member_joined_count = Arc::new(AtomicUsize::new(0));
        let any_member_left_count = Arc::new(AtomicUsize::new(0));
        let any_member_offline_count = Arc::new(AtomicUsize::new(0));
        let any_member_online_count = Arc::new(AtomicUsize::new(0));
        let group_leader_changed_count = Arc::new(AtomicUsize::new(0));
        let group_member_joined_count = Arc::new(AtomicUsize::new(0));
        let group_member_left_count = Arc::new(AtomicUsize::new(0));
        let group_member_online_count = Arc::new(AtomicUsize::new(0));
        let group_member_offline_count = Arc::new(AtomicUsize::new(0));

        let any_member_joined_count_clone = any_member_joined_count.clone();
        let any_member_left_count_clone = any_member_left_count.clone();
        let any_member_offline_count_clone = any_member_offline_count.clone();
        let any_member_online_count_clone = any_member_online_count.clone();
        let group_leader_changed_count_clone = group_leader_changed_count.clone();
        let group_member_joined_count_clone = group_member_joined_count.clone();
        let group_member_left_count_clone = group_member_left_count.clone();
        let group_member_online_count_clone = group_member_online_count.clone();
        let group_member_offline_count_clone = group_member_offline_count.clone();

        client
            .on_any_member_joined(move |res| {
                any_member_joined_count_clone.fetch_add(1, Ordering::Relaxed);
                future::ready(()).boxed()
            })
            .await
            .unwrap()
            .unwrap();

        client
            .on_any_member_left(move |res| {
                any_member_left_count_clone.fetch_add(1, Ordering::Relaxed);
                future::ready(()).boxed()
            })
            .await
            .unwrap()
            .unwrap();

        client
            .on_any_member_offline(move |res| {
                any_member_offline_count_clone.fetch_add(1, Ordering::Relaxed);
                future::ready(()).boxed()
            })
            .await
            .unwrap()
            .unwrap();

        client
            .on_any_member_online(move |res| {
                any_member_online_count_clone.fetch_add(1, Ordering::Relaxed);
                future::ready(()).boxed()
            })
            .await
            .unwrap()
            .unwrap();

        client
            .on_group_leader_changed(
                move |res| {
                    group_leader_changed_count_clone.fetch_add(1, Ordering::Relaxed);
                    future::ready(()).boxed()
                },
                &group_1,
            )
            .await
            .unwrap()
            .unwrap();

        client
            .on_group_member_joined(
                move |res| {
                    group_member_joined_count_clone.fetch_add(1, Ordering::Relaxed);
                    future::ready(()).boxed()
                },
                &group_1,
            )
            .await
            .unwrap()
            .unwrap();

        client
            .on_group_member_left(
                move |res| {
                    group_member_left_count_clone.fetch_add(1, Ordering::Relaxed);
                    future::ready(()).boxed()
                },
                &group_1,
            )
            .await
            .unwrap()
            .unwrap();

        client
            .on_group_member_online(
                move |res| {
                    group_member_online_count_clone.fetch_add(1, Ordering::Relaxed);
                    future::ready(()).boxed()
                },
                &group_1,
            )
            .await
            .unwrap()
            .unwrap();

        client
            .on_group_member_offline(
                move |res| {
                    group_member_offline_count_clone.fetch_add(1, Ordering::Relaxed);
                    future::ready(()).boxed()
                },
                &group_1,
            )
            .await
            .unwrap()
            .unwrap();

        let member1_raft_client = RaftClient::new(&vec![addr.clone()], DEFAULT_SERVICE_ID)
            .await
            .unwrap();
        let member1_addr = String::from("server1");
        let member1_svr = MemberService::new(&member1_addr, &member1_raft_client).await;

        let member2_raft_client = RaftClient::new(&vec![addr.clone()], DEFAULT_SERVICE_ID)
            .await
            .unwrap();
        let member2_addr = String::from("server2");
        let member2_svr = MemberService::new(&member2_addr, &member2_raft_client).await;

        let member3_raft_client = RaftClient::new(&vec![addr.clone()], DEFAULT_SERVICE_ID)
            .await
            .unwrap();
        let member3_addr = String::from("server3");
        let member3_svr = MemberService::new(&member3_addr, &member3_raft_client).await;

        member1_svr.join_group(&group_1).await.unwrap();
        member2_svr.join_group(&group_1).await.unwrap();
        member3_svr.join_group(&group_1).await.unwrap();

        member1_svr.join_group(&group_2).await.unwrap();
        member2_svr.join_group(&group_2).await.unwrap();

        member1_svr.join_group(&group_3).await.unwrap();

        assert_eq!(
            member1_svr
                .client()
                .all_members(false)
                .await
                .unwrap()
                .0
                .len(),
            3
        );
        assert_eq!(
            member1_svr
                .client()
                .all_members(true)
                .await
                .unwrap()
                .0
                .len(),
            3
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_1, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            3
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_1, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            3
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_2, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            2
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_2, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            2
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_3, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            1
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_3, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            1
        );

        member1_svr.close(); // close only end the heartbeat thread

        info!("Waiting for membership changes");
        async_wait_secs().await;
        async_wait_secs().await;
        info!("Checking members");

        assert_eq!(
            member1_svr
                .client()
                .all_members(false)
                .await
                .unwrap()
                .0
                .len(),
            3
        );
        assert_eq!(
            member1_svr
                .client()
                .all_members(true)
                .await
                .unwrap()
                .0
                .len(),
            2
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_1, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            3
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_1, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            2
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_2, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            2
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_2, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            1
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_3, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            1
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_3, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            0
        );

        member2_svr.leave().await.unwrap(); // leave will report to the raft servers to remove it from the list
        assert_eq!(
            member1_svr
                .client()
                .all_members(false)
                .await
                .unwrap()
                .0
                .len(),
            2
        );
        assert_eq!(
            member1_svr
                .client()
                .all_members(true)
                .await
                .unwrap()
                .0
                .len(),
            1
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_1, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            2
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_1, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            1
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_2, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            1
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_2, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            0
        );

        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_3, false)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            1
        );
        assert_eq!(
            member1_svr
                .client()
                .group_members(&group_3, true)
                .await
                .unwrap()
                .unwrap()
                .0
                .len(),
            0
        );

        async_wait_secs().await;

        info!("Checking event trigger");
        assert_eq!(any_member_joined_count.load(Ordering::Relaxed), 3);
        assert_eq!(any_member_left_count.load(Ordering::Relaxed), 1);
        assert_eq!(any_member_offline_count.load(Ordering::Relaxed), 1);
        assert_eq!(any_member_online_count.load(Ordering::Relaxed), 0); // no server online from offline
        assert!(group_leader_changed_count.load(Ordering::Relaxed) > 0); // Number depends on hashing
        assert_eq!(group_member_joined_count.load(Ordering::Relaxed), 3);
        // assert_eq!(group_member_left_count.load(Ordering::Relaxed), 2); // this test case is unstable
        assert_eq!(group_member_online_count.load(Ordering::Relaxed), 0);
        assert_eq!(group_member_offline_count.load(Ordering::Relaxed), 1);
    }
}
