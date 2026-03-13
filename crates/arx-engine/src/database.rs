use arx_core::error::Error;

use crate::container::ContainerManager;

pub struct DatabaseManager<'a> {
    containers: &'a ContainerManager,
}

pub struct DatabaseInfo {
    pub container_id: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl<'a> DatabaseManager<'a> {
    pub fn new(containers: &'a ContainerManager) -> Self {
        Self { containers }
    }

    pub async fn provision(
        &self,
        engine: &str,
        project_id: &str,
        db_name: &str,
    ) -> Result<DatabaseInfo, Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let password = generate_password();
        let username = match engine {
            "postgres" => "postgres",
            "mysql" => "root",
            "redis" => "",
            _ => return Err(Error::Internal(format!("unsupported engine: {engine}"))),
        };

        let (image, port, env) = match engine {
            "postgres" => (
                "postgres:16",
                5432u16,
                vec![
                    format!("POSTGRES_DB={db_name}"),
                    format!("POSTGRES_PASSWORD={password}"),
                ],
            ),
            "mysql" => (
                "mysql:8",
                3306u16,
                vec![
                    format!("MYSQL_DATABASE={db_name}"),
                    format!("MYSQL_ROOT_PASSWORD={password}"),
                ],
            ),
            "redis" => ("redis:7", 6379u16, vec![]),
            _ => unreachable!(),
        };

        let container_name = format!("arx-db-{}-{}", project_id, &id[..8]);
        let network_name = format!("arx-{project_id}");
        let volume_path = format!("/var/lib/arx/data/db-{id}");
        let volume_mount = match engine {
            "postgres" => format!("{volume_path}:/var/lib/postgresql/data"),
            "mysql" => format!("{volume_path}:/var/lib/mysql"),
            "redis" => format!("{volume_path}:/data"),
            _ => unreachable!(),
        };

        self.containers.pull_image(image).await?;
        self.containers.create_network(&network_name).await?;

        let container_id = self
            .containers
            .create_and_start(
                &container_name,
                image,
                port,
                env,
                None,
                None,
                Some(&network_name),
                Some(vec![volume_mount]),
            )
            .await?;

        let host_port = self.containers.get_host_port(&container_id, port).await?;

        Ok(DatabaseInfo {
            container_id,
            port: host_port,
            username: username.to_string(),
            password,
        })
    }

    pub async fn destroy(&self, container_id: &str) -> Result<(), Error> {
        self.containers.stop_and_remove(container_id).await
    }

    pub async fn status(&self, container_id: &str) -> bool {
        self.containers.is_running(container_id).await
    }
}

fn generate_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..24).map(|_| rng.gen()).collect();
    hex::encode(bytes)
}
