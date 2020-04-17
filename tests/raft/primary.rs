use super::wait;
use bifrost::raft::state_machine::master::ExecError;
use bifrost::raft::*;
use bifrost::rpc::Server;
use std::fs::File;

#[tokio::test(threaded_scheduler)]
async fn startup() {
    let (success, _, _) = RaftService::new_server(Options {
        storage: Storage::default(),
        address: String::from("127.0.0.1:2000"),
        service_id: DEFAULT_SERVICE_ID,
    })
    .await;
    assert!(success);
}

#[tokio::test(threaded_scheduler)]
async fn server_membership() {
    let s1_addr = String::from("127.0.0.1:2001");
    let s2_addr = String::from("127.0.0.1:2002");
    let s3_addr = String::from("127.0.0.1:2003");
    let service1 = RaftService::new(Options {
        storage: Storage::default(),
        address: s1_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });
    let server1 = Server::new(&s1_addr);
    server1.register_service(DEFAULT_SERVICE_ID, &service1);
    Server::listen_and_resume(&server1);
    assert!(RaftService::start(&service1).await);
    service1.bootstrap().await;
    assert_eq!(service1.num_members().await, 1);
    let service2 = RaftService::new(Options {
        storage: Storage::default(),
        address: s2_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });
    let server2 = Server::new(&s2_addr);
    server2.register_service(DEFAULT_SERVICE_ID, &service2);
    Server::listen_and_resume(&server2);
    assert!(RaftService::start(&service2).await);
    let join_result = service2.join(&vec![s1_addr.clone()]).await;
    match join_result {
        Err(ExecError::ServersUnreachable) => panic!("Server unreachable"),
        Err(ExecError::CannotConstructClient) => panic!("Cannot Construct Client"),
        Err(e) => panic!(e),
        Ok(_) => {}
    }
    assert!(join_result.is_ok());
    assert_eq!(service1.num_members().await, 2);
    assert_eq!(service2.num_members().await, 2);
    let service3 = RaftService::new(Options {
        storage: Storage::default(),
        address: s3_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });
    let server3 = Server::new(&s3_addr);
    server3.register_service(DEFAULT_SERVICE_ID, &service3);
    Server::listen_and_resume(&server3);
    assert!(RaftService::start(&service3).await);
    let join_result = service3.join(&vec![s1_addr.clone(), s2_addr.clone()]).await;
    join_result.unwrap();
    assert_eq!(service1.num_members().await, 3);
    assert_eq!(service3.num_members().await, 3);

    wait().await;

    // check in service2. Although it is a log replication problem but membership changes should take effect immediately
    assert_eq!(service2.num_members().await, 3);

    // test remove member
    assert!(service2.leave().await);
    assert_eq!(service1.num_members().await, 2);
    assert_eq!(service3.num_members().await, 2);

    //test remove leader
    assert_eq!(service1.leader_id(), service1.id);
    assert!(service1.leave().await);
    wait().await; // there will be some unavailability in leader transaction
    assert_eq!(service3.leader_id(), service3.id);
    assert_eq!(service3.num_members().await, 1);
}

#[tokio::test(threaded_scheduler)]
async fn log_replication() {
    let s1_addr = String::from("127.0.0.1:2004");
    let s2_addr = String::from("127.0.0.1:2005");
    let s3_addr = String::from("127.0.0.1:2006");
    let s4_addr = String::from("127.0.0.1:2007");
    let s5_addr = String::from("127.0.0.1:2008");
    let service1 = RaftService::new(Options {
        storage: Storage::default(),
        address: s1_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });
    let service2 = RaftService::new(Options {
        storage: Storage::default(),
        address: s2_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });
    let service3 = RaftService::new(Options {
        storage: Storage::default(),
        address: s3_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });
    let service4 = RaftService::new(Options {
        storage: Storage::default(),
        address: s4_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });
    let service5 = RaftService::new(Options {
        storage: Storage::default(),
        address: s5_addr.clone(),
        service_id: DEFAULT_SERVICE_ID,
    });

    let server1 = Server::new(&s1_addr);
    server1.register_service(DEFAULT_SERVICE_ID, &service1);
    Server::listen_and_resume(&server1);
    assert!(RaftService::start(&service1).await);
    service1.bootstrap().await;

    let server2 = Server::new(&s2_addr);
    server2.register_service(DEFAULT_SERVICE_ID, &service2);
    Server::listen_and_resume(&server2);
    assert!(RaftService::start(&service2).await);
    let join_result = service2.join(&vec![s1_addr.clone(), s2_addr.clone()]).await;
    join_result.unwrap();

    let server3 = Server::new(&s3_addr);
    server3.register_service(DEFAULT_SERVICE_ID, &service3);
    Server::listen_and_resume(&server3);
    assert!(RaftService::start(&service3).await);
    let join_result = service3.join(&vec![s1_addr.clone(), s2_addr.clone()]).await;
    join_result.unwrap();

    let server4 = Server::new(&s4_addr);
    server4.register_service(DEFAULT_SERVICE_ID, &service4);
    Server::listen_and_resume(&server4);
    assert!(RaftService::start(&service4).await);
    let join_result = service4
        .join(&vec![s1_addr.clone(), s2_addr.clone(), s3_addr.clone()])
        .await;
    join_result.unwrap();

    let server5 = Server::new(&s5_addr);
    server5.register_service(DEFAULT_SERVICE_ID, &service5);
    Server::listen_and_resume(&server5);
    assert!(RaftService::start(&service5).await);
    let join_result = service5
        .join(&vec![
            s1_addr.clone(),
            s2_addr.clone(),
            s3_addr.clone(),
            s4_addr.clone(),
        ])
        .await;
    join_result.unwrap();

    wait().await; // wait for membership replication to take effect

    assert_eq!(service1.num_logs().await, service2.num_logs().await);
    assert_eq!(service2.num_logs().await, service3.num_logs().await);
    assert_eq!(service3.num_logs().await, service4.num_logs().await);
    assert_eq!(service4.num_logs().await, service5.num_logs().await);
    assert_eq!(service5.num_logs().await, 4); // check all logs replicated

    wait().await;

    assert_eq!(service1.leader_id().await, service1.id);
    assert_eq!(service2.leader_id().await, service1.id);
    assert_eq!(service3.leader_id().await, service1.id);
    assert_eq!(service4.leader_id().await, service1.id);
    assert_eq!(service5.leader_id().await, service1.id);
}
