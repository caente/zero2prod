use reqwest;
use secrecy::SecretBox;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use std::sync::LazyLock;
use uuid::Uuid;
use zero2prod::configuration::{DatabaseSettings, get_configuration};
use zero2prod::email_client::EmailClient;
use zero2prod::startup::run;
use zero2prod::telemetry;

static TRACING: LazyLock<()> = LazyLock::new(|| {
    let subscriber_name = "zero2prod_tests".into();
    let level = "debug".into();
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = telemetry::get_subscriber(subscriber_name, level, std::io::stdout);
        telemetry::init_subscriber(subscriber);
    } else {
        let subscriber = telemetry::get_subscriber(subscriber_name, level, std::io::sink);
        telemetry::init_subscriber(subscriber);
    }
});

#[tokio::test]
async fn health_check_works() {
    let TestApp { address, db_pool } = spawn_app().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health_check", &address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_returns_200_for_valid_form_data() {
    let TestApp { address, db_pool } = spawn_app().await;
    let client = reqwest::Client::new();
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let response = client
        .post(&format!("{}/subscribe", &address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");
    assert_eq!(200, response.status().as_u16());
    let saved = sqlx::query!(r#"SELECT email, name FROM subscriptions"#,)
        .fetch_one(&db_pool)
        .await
        .expect("Failed to fetch");
    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
}

#[tokio::test]
async fn subscribe_returns_400_when_data_is_missing() {
    let TestApp { address, db_pool } = spawn_app().await;
    let client = reqwest::Client::new();
    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%30gmail.com", "missing the name"),
        ("", "missing name and email"),
    ];
    for (invalid_body, error_message) in test_cases {
        let response = client
            .post(&format!("{}/subscribe", &address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(
            400,
            response.status().as_u16(),
            "There should be failure for payload {}",
            error_message
        );
    }
}

#[tokio::test]
async fn subscribe_returns_400_when_fields_are_present_but_empty() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();
    let test_cases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("name=Ursula@email=", "empty email"),
        ("name=Ursula&email=bad-email", "invalid email"),
    ];
    for (body, description) in test_cases {
        let response = client
            .post(&format!("{}/subscribe", app.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(
            400,
            response.status().as_u16(),
            "Did not return 400: {}",
            description
        )
    }
}

struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}
async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);
    let mut configuration = get_configuration().expect("Failed to read conf");
    configuration.database.database_name = Uuid::new_v4().to_string();
    let sender_email = configuration.email_client.sender().expect("Wrong email");
    let email_client = EmailClient::new(configuration.email_client.base_url, sender_email);
    let db_pool = configure_database(&configuration.database).await;
    let listener = TcpListener::bind("127.0.0.1:0").expect("Port unavailable");
    let port = listener
        .local_addr()
        .expect("Local address not found")
        .port();
    let server = run(listener, db_pool.clone(), email_client).expect("Failed to start server");
    let _ = tokio::spawn(server);
    let address = format!("http://127.0.0.1:{}", port);
    TestApp { address, db_pool }
}

pub async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(),
        username: "postgres".to_string(),
        password: SecretBox::new(Box::new("postgres".to_string())),
        host: config.host.clone(),
        port: config.port,
        require_ssl: false,
    };
    let mut connection = PgConnection::connect_with(&maintenance_settings.connection_options())
        .await
        .expect("Failed to connect to postgres");
    connection
        .execute(format!(r#"CREATE DATABASE "{}"; "#, config.database_name).as_str())
        .await
        .expect("Failed to create the database");
    let connection_pool = PgPool::connect_with(config.connection_options())
        .await
        .expect("Failed to connect to postgres");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate");
    connection_pool
}
