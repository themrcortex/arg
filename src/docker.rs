use super::*;

#[derive(Deserialize, Default)]
pub(super) struct DockerOperation {
    action: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    ports: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    network: String,
    #[serde(default)]
    restart: String,
    #[serde(default)]
    compose_file: String,
    #[serde(default)]
    context_path: String,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    confirmed: bool,
    #[serde(default)]
    application_id: Option<String>,
    #[serde(default)]
    memory_limit_mb: Option<u64>,
    #[serde(default)]
    cpu_limit: Option<f64>,
    #[serde(default)]
    internal: bool,
    #[serde(default = "default_log_retention_days")]
    retention_days: u16,
}

fn default_log_retention_days() -> u16 {
    30
}

#[derive(Deserialize, Default)]
pub(super) struct DockerOptimizationQuery {
    #[serde(default = "default_log_retention_days")]
    retention_days: u16,
}

pub(super) async fn docker_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let result = privileged_request_async(json!({"action":"docker-status"})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            result["error"]
                .as_str()
                .unwrap_or("Docker status is unavailable"),
        ));
    }
    Ok(Json(result["data"].clone()))
}

pub(super) async fn docker_optimization_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DockerOptimizationQuery>,
) -> ApiResult {
    if ![7, 30, 90].contains(&query.retention_days) {
        return Err(error(StatusCode::BAD_REQUEST, "مدت نگه‌داری لاگ معتبر نیست."));
    }
    let protected_cache_mount_ids = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
        let mut statement = db
            .prepare("SELECT id FROM s3_mounts WHERE status != 'unmounted'")
            .map_err(db_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        ids
    };
    let result = privileged_request_async(json!({"action":"docker-optimization-status","protected_cache_mount_ids":protected_cache_mount_ids,"retention_days":query.retention_days})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            result["error"]
                .as_str()
                .unwrap_or("وضعیت بهینه‌سازی سرور دریافت نشد."),
        ));
    }
    Ok(Json(result["data"].clone()))
}
pub(super) async fn docker_operation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<DockerOperation>,
) -> ApiResult {
    check_origin(&headers)?;
    let actor = {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?.username
    };
    let protected_cache_mount_ids = if input.action == "arg-cache-prune" {
        let db = state.lock().unwrap();
        let mut statement = db
            .prepare("SELECT id FROM s3_mounts WHERE status != 'unmounted'")
            .map_err(db_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        ids
    } else {
        Vec::new()
    };
    if let Some(application_id) = input.application_id.as_deref() {
        if input.action != "container-run" || !apps::valid_uuid(application_id) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "اتصال کانتینر فقط با شناسهٔ معتبر اپ مجاز است.",
            ));
        }
        let db = state.lock().unwrap();
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        let owned:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND source_type='docker' AND deleted_at IS NULL)",params![application_id,owner],|r|r.get(0)).map_err(db_error)?;
        if !owned {
            return Err(error(
                StatusCode::NOT_FOUND,
                "اپلیکیشن Docker متعلق به این میزبان پیدا نشد.",
            ));
        }
    }
    let allowed = [
        "install",
        "uninstall",
        "start",
        "stop",
        "restart",
        "enable",
        "disable",
        "container-run",
        "container-start",
        "container-stop",
        "container-restart",
        "container-pause",
        "container-unpause",
        "container-remove",
        "container-logs",
        "container-inspect",
        "container-exec",
        "image-pull",
        "image-push",
        "image-build",
        "image-tag",
        "image-remove",
        "image-prune",
        "network-create",
        "network-remove",
        "volume-create",
        "volume-remove",
        "volume-prune",
        "system-prune",
        "compose-up",
        "compose-down",
        "compose-pull",
        "docker-builder-prune",
        "docker-container-prune",
        "docker-network-prune",
        "arg-cache-prune",
        "server-logs-prune",
        "apt-cache-clean",
    ];
    if !allowed.contains(&input.action.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Docker operation is invalid",
        ));
    }
    if [
        "uninstall",
        "container-remove",
        "image-remove",
        "image-push",
        "network-remove",
        "volume-remove",
        "image-prune",
        "volume-prune",
        "system-prune",
        "docker-builder-prune",
        "docker-container-prune",
        "docker-network-prune",
        "arg-cache-prune",
        "server-logs-prune",
        "apt-cache-clean",
        "stop",
        "restart",
        "disable",
        "compose-down",
    ]
    .contains(&input.action.as_str())
        && !input.confirmed
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Confirm this Docker operation before continuing",
        ));
    }
    let _update_guard = UPDATE_OPERATION_GATE.read().await;
    if input.action == "server-logs-prune" && ![7, 30, 90].contains(&input.retention_days) {
        return Err(error(StatusCode::BAD_REQUEST, "مدت نگه‌داری لاگ معتبر نیست."));
    }
    let result = privileged_request_async(json!({"action":"docker-operation","operation":input.action,"target":input.target,"name":input.name,"image":input.image,"ports":input.ports,"env":input.env,"network":input.network,"restart":input.restart,"compose_file":input.compose_file,"context_path":input.context_path,"command":input.command,"confirmed":input.confirmed,"application_id":input.application_id,"memory_limit_mb":input.memory_limit_mb,"cpu_limit":input.cpu_limit,"internal":input.internal,"retention_days":input.retention_days,"protected_cache_mount_ids":protected_cache_mount_ids})).await?;
    if result["ok"] != true {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            result["error"]
                .as_str()
                .unwrap_or("Docker operation failed"),
        ));
    }
    {
        let db = state.lock().unwrap();
        audit(&db, &actor, "docker.operation", &input.action);
    }
    Ok(Json(
        json!({"output":result["stdout"].as_str().unwrap_or("Docker operation completed.")}),
    ))
}

fn docker_table(args: &[&str], fields: &[&str]) -> Result<Vec<Value>, String> {
    let mut command = std::process::Command::new("docker");
    command.args(args);
    let (success, output) = command_result(&mut command)?;
    if !success {
        return Err(output);
    }
    Ok(output
        .lines()
        .filter_map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            if values.len() != fields.len() {
                return None;
            }
            Some(Value::Object(
                fields
                    .iter()
                    .zip(values)
                    .map(|(key, value)| ((*key).to_owned(), json!(value)))
                    .collect(),
            ))
        })
        .collect())
}

pub(super) fn execute_docker_status() -> Value {
    let mut client = std::process::Command::new("docker");
    client.arg("--version");
    let client_result = command_result(&mut client);
    let Ok((true, client_version)) = client_result else {
        return json!({"ok":true,"data":{"installed":false,"active":false,"version":"","containers":[],"images":[],"networks":[],"volumes":[],"error":"Docker is not installed."}});
    };
    let mut server = std::process::Command::new("docker");
    server.args(["version", "--format", "{{.Server.Version}}"]);
    let server_result = command_result(&mut server);
    let active = matches!(&server_result, Ok((true, _)));
    if !active {
        let detail = server_result
            .map(|(_, output)| output)
            .unwrap_or_else(|error| error);
        return json!({"ok":true,"data":{"installed":true,"active":false,"version":client_version,"server_version":"","containers":[],"images":[],"networks":[],"volumes":[],"error":detail}});
    }
    let server_version = server_result.unwrap().1;
    let mut containers = docker_table(
        &[
            "ps",
            "-a",
            "--no-trunc",
            "--format",
            "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}\t{{.State}}\t{{.Label \"com.arg.application_id\"}}",
        ],
        &["id", "names", "image", "status", "ports", "state", "application_id"],
    );
    if let Ok(stats) = docker_table(
        &[
            "stats",
            "--no-stream",
            "--format",
            "{{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}\t{{.NetIO}}\t{{.BlockIO}}",
        ],
        &[
            "container",
            "cpu",
            "memory",
            "memory_percent",
            "network_io",
            "block_io",
        ],
    ) {
        if let Ok(rows) = containers.as_mut() {
            for container in rows {
                if let Some(id) = container["id"].as_str() {
                    if let Some(stat) = stats.iter().find(|stat| {
                        stat["container"]
                            .as_str()
                            .is_some_and(|key| id.starts_with(key))
                    }) {
                        container["stats"] = stat.clone();
                    }
                }
            }
        }
    }
    let images = docker_table(
        &[
            "image",
            "ls",
            "-a",
            "--no-trunc",
            "--format",
            "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}",
        ],
        &["id", "repository", "tag", "size", "created"],
    );
    let mut networks = docker_table(
        &[
            "network",
            "ls",
            "--no-trunc",
            "--format",
            "{{.ID}}\t{{.Name}}\t{{.Driver}}\t{{.Scope}}",
        ],
        &["id", "name", "driver", "scope"],
    );
    if let Ok(rows) = networks.as_mut() {
        let names: Vec<&str> = rows
            .iter()
            .filter_map(|network| network["name"].as_str())
            .collect();
        if !names.is_empty() {
            let mut inspect = std::process::Command::new("docker");
            inspect.args(["network", "inspect"]).args(&names);
            if let Ok((true, output)) = command_result(&mut inspect) {
                if let Ok(details) = serde_json::from_str::<Vec<Value>>(&output) {
                    for network in rows {
                        let name = network["name"].as_str().unwrap_or_default();
                        let detail = details.iter().find(|item| item["Name"] == name);
                        let members = detail
                            .and_then(|item| item["Containers"].as_object())
                            .map(|attached| {
                                attached
                                    .iter()
                                    .map(|(container_id, item)| {
                                        let container = containers
                                            .as_ref()
                                            .ok()
                                            .and_then(|items| {
                                                items.iter().find(|container| {
                                                    container["id"]
                                                        .as_str()
                                                        .is_some_and(|id| id.starts_with(container_id))
                                                })
                                            });
                                        json!({
                                            "id": container_id,
                                            "name": item["Name"],
                                            "ipv4": item["IPv4Address"],
                                            "ipv6": item["IPv6Address"],
                                            "application_id": container.and_then(|container| container["application_id"].as_str())
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        network["internal"] = json!(detail
                            .and_then(|item| item["Internal"].as_bool())
                            .unwrap_or(false));
                        network["containers"] = json!(members);
                        network["details_available"] = json!(detail.is_some());
                    }
                }
            }
        }
    }
    let volumes = docker_table(
        &[
            "volume",
            "ls",
            "--format",
            "{{.Name}}\t{{.Driver}}\t{{.Mountpoint}}",
        ],
        &["name", "driver", "mountpoint"],
    );
    let mut compose_version_command = std::process::Command::new("docker");
    compose_version_command.args(["compose", "version", "--short"]);
    let compose_version = command_result(&mut compose_version_command)
        .ok()
        .filter(|(success, _)| *success)
        .map(|(_, output)| output)
        .unwrap_or_default();
    let mut compose_list_command = std::process::Command::new("docker");
    compose_list_command.args(["compose", "ls", "--all", "--format", "json"]);
    let compose_projects: Vec<Value> = command_result(&mut compose_list_command)
        .ok()
        .and_then(|(success, output)| {
            if success {
                serde_json::from_str(&output).ok()
            } else {
                None
            }
        })
        .unwrap_or_default();
    let failed = [&containers, &images, &networks, &volumes]
        .iter()
        .find_map(|result| result.as_ref().err().cloned());
    json!({"ok":true,"data":{"installed":true,"active":true,"version":client_version,"server_version":server_version,"compose_version":compose_version,"compose_projects":compose_projects,"containers":containers.unwrap_or_default(),"images":images.unwrap_or_default(),"networks":networks.unwrap_or_default(),"volumes":volumes.unwrap_or_default(),"error":failed.unwrap_or_default()}})
}

pub(super) fn valid_docker_object(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
        && !value.contains("..")
}

pub(super) fn valid_docker_image(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._/:@-".contains(&byte))
}

pub(super) fn valid_docker_port(value: &str) -> bool {
    let (mapping, protocol) = value.split_once('/').unwrap_or((value, "tcp"));
    if !["tcp", "udp"].contains(&protocol) {
        return false;
    }
    let Some((host, container)) = mapping.split_once(':') else {
        return false;
    };
    host.parse::<u16>().is_ok_and(|port| port > 0)
        && container.parse::<u16>().is_ok_and(|port| port > 0)
}

pub(super) fn execute_docker_operation(request: &Value) -> Value {
    let Some(operation) = request["operation"].as_str() else {
        return json!({"ok":false,"error":"Missing Docker operation"});
    };
    let confirmed = request["confirmed"].as_bool().unwrap_or(false);
    let target = request["target"].as_str().unwrap_or_default();
    let name = request["name"].as_str().unwrap_or_default();
    let image = request["image"].as_str().unwrap_or_default();
    let application_id = request["application_id"].as_str();
    let mut args: Vec<String> = Vec::new();
    match operation {
        "install" => {
            let mut update = std::process::Command::new("apt-get");
            update.args(["update"]);
            if let Err(error) = command_result(&mut update).and_then(|(ok, output)| {
                if ok {
                    Ok((ok, output))
                } else {
                    Err(output)
                }
            }) {
                return json!({"ok":false,"error":error});
            }
            let mut install = std::process::Command::new("apt-get");
            install.args(["install", "-y", "docker.io"]);
            return match command_result(&mut install) {
                Ok((true, output)) => {
                    let mut enable = std::process::Command::new("systemctl");
                    enable.args(["enable", "--now", "docker"]);
                    match command_result(&mut enable) {
                        Ok((true, service)) => {
                            json!({"ok":true,"stdout":format!("{output}\n{service}")})
                        }
                        Ok((false, service)) => json!({"ok":false,"error":service}),
                        Err(error) => json!({"ok":false,"error":error}),
                    }
                }
                Ok((false, output)) => json!({"ok":false,"error":output}),
                Err(error) => json!({"ok":false,"error":error}),
            };
        }
        "uninstall" => {
            if !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            let mut stop = std::process::Command::new("systemctl");
            stop.args(["disable", "--now", "docker"]);
            let stop_result = command_result(&mut stop)
                .map(|(_, output)| output)
                .unwrap_or_default();
            let mut purge = std::process::Command::new("apt-get");
            purge.args(["purge", "-y", "docker.io"]);
            return match command_result(&mut purge) {
                Ok((true, output)) => {
                    json!({"ok":true,"stdout":format!("{stop_result}\n{output}\nDocker data directory was not explicitly removed.")})
                }
                Ok((false, output)) => json!({"ok":false,"error":output}),
                Err(error) => json!({"ok":false,"error":error}),
            };
        }
        "start" | "stop" | "restart" | "enable" | "disable" => {
            if ["stop", "restart", "disable"].contains(&operation) && !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            args.extend([
                "systemctl".to_owned(),
                operation.to_owned(),
                "docker".to_owned(),
            ]);
            let mut command = std::process::Command::new(&args[0]);
            command.args(&args[1..]);
            return match command_result(&mut command) {
                Ok((true, output)) => json!({"ok":true,"stdout":output}),
                Ok((false, output)) => json!({"ok":false,"error":output}),
                Err(error) => json!({"ok":false,"error":error}),
            };
        }
        "container-run" => {
            if !valid_docker_object(name) || !valid_docker_image(image) {
                return json!({"ok":false,"error":"Invalid container name or image"});
            }
            let ports: Vec<&str> = request["ports"]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let envs: Vec<&str> = request["env"]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if ports.len() > 32
                || ports.iter().any(|port| !valid_docker_port(port))
                || envs.len() > 64
                || envs.iter().any(|value| {
                    value.len() > 4096
                        || value.bytes().any(|byte| matches!(byte, b'\n' | b'\r'))
                        || !value.split_once('=').is_some_and(|(key, _)| {
                            !key.is_empty()
                                && key.bytes().enumerate().all(|(i, byte)| {
                                    if i == 0 {
                                        byte.is_ascii_alphabetic() || byte == b'_'
                                    } else {
                                        byte.is_ascii_alphanumeric() || byte == b'_'
                                    }
                                })
                        })
                })
            {
                return json!({"ok":false,"error":"Invalid port or environment settings"});
            }
            let restart = request["restart"].as_str().unwrap_or("unless-stopped");
            if !["no", "always", "unless-stopped", "on-failure"].contains(&restart) {
                return json!({"ok":false,"error":"Invalid restart policy"});
            }
            args.extend([
                "run".into(),
                "-d".into(),
                "--name".into(),
                name.into(),
                "--restart".into(),
                restart.into(),
            ]);
            if let Some(application_id) = application_id {
                if !apps::valid_uuid(application_id) {
                    return json!({"ok":false,"error":"Invalid ARG application id"});
                }
                args.extend([
                    "--label".into(),
                    format!("com.arg.application_id={application_id}"),
                ]);
            }
            for port in ports {
                args.extend(["-p".into(), port.into()]);
            }
            for value in envs {
                args.extend(["--env".into(), value.into()]);
            }
            if let Some(memory_mb) = request["memory_limit_mb"].as_u64() {
                if !(16..=1_048_576).contains(&memory_mb) {
                    return json!({"ok":false,"error":"Memory limit must be between 16 MB and 1 TB"});
                }
                args.extend(["--memory".into(), format!("{memory_mb}m")]);
            }
            if !request["memory_limit_mb"].is_null()
                && request["memory_limit_mb"].as_u64().is_none()
            {
                return json!({"ok":false,"error":"Invalid memory limit"});
            }
            if let Some(cpu_limit) = request["cpu_limit"].as_f64() {
                if !cpu_limit.is_finite() || !(0.1..=128.0).contains(&cpu_limit) {
                    return json!({"ok":false,"error":"CPU limit must be between 0.1 and 128 cores"});
                }
                args.extend(["--cpus".into(), cpu_limit.to_string()]);
            }
            if !request["cpu_limit"].is_null() && request["cpu_limit"].as_f64().is_none() {
                return json!({"ok":false,"error":"Invalid CPU limit"});
            }
            let network = request["network"].as_str().unwrap_or_default();
            if !network.is_empty() {
                if !valid_docker_object(network) {
                    return json!({"ok":false,"error":"Invalid Docker network"});
                }
                args.extend(["--network".into(), network.into()]);
            }
            args.push(image.into());
        }
        "container-start" | "container-stop" | "container-restart" | "container-pause"
        | "container-unpause" | "container-remove" | "container-logs" | "container-inspect" => {
            if !valid_docker_object(target) {
                return json!({"ok":false,"error":"Invalid container identifier"});
            }
            if operation == "container-remove" && !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            let action = match operation {
                "container-start" => "start",
                "container-stop" => "stop",
                "container-restart" => "restart",
                "container-pause" => "pause",
                "container-unpause" => "unpause",
                "container-remove" => "rm",
                "container-logs" => "logs",
                "container-inspect" => "inspect",
                _ => unreachable!(),
            };
            args.push(action.into());
            if operation == "container-remove" {
                args.push("-f".into());
            }
            if operation == "container-logs" {
                args.extend(["--tail".into(), "200".into(), "--timestamps".into()]);
            }
            if operation == "container-inspect" {
                args.extend(["--type".into(), "container".into()]);
            }
            args.push(target.into());
        }
        "container-exec" => {
            if !valid_docker_object(target) {
                return json!({"ok":false,"error":"Invalid container identifier"});
            }
            let command: Vec<&str> = request["command"]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if command.is_empty()
                || command.len() > 32
                || command.iter().any(|part| {
                    part.is_empty()
                        || part.len() > 1024
                        || part.bytes().any(|byte| matches!(byte, b'\n' | b'\r'))
                })
            {
                return json!({"ok":false,"error":"Enter a valid command and arguments"});
            }
            args.extend(["exec".into(), "-i".into(), target.into()]);
            args.extend(command.iter().map(|part| (*part).to_owned()));
        }
        "image-pull" | "image-push" | "image-remove" => {
            let reference = if target.is_empty() { image } else { target };
            if !valid_docker_image(reference)
                || ["image-remove", "image-push"].contains(&operation) && !confirmed
            {
                return json!({"ok":false,"error":"Invalid image reference or missing confirmation"});
            }
            args.extend([
                "image".into(),
                match operation {
                    "image-pull" => "pull",
                    "image-push" => "push",
                    _ => "rm",
                }
                .into(),
            ]);
            if operation == "image-remove" {
                args.push("--".into());
            }
            args.push(reference.into());
        }
        "image-build" => {
            let Some(context) = request["context_path"].as_str().filter(|context| {
                let path = std::path::Path::new(context);
                path.is_absolute()
                    && !path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                    && path.is_dir()
                    && path.join("Dockerfile").is_file()
            }) else {
                return json!({"ok":false,"error":"Build context must be an absolute directory containing Dockerfile"});
            };
            if !valid_docker_image(image) {
                return json!({"ok":false,"error":"Invalid image tag"});
            }
            args.extend(["build".into(), "--tag".into(), image.into(), context.into()]);
        }
        "image-tag" => {
            if !valid_docker_image(target) || !valid_docker_image(image) {
                return json!({"ok":false,"error":"Invalid source or target image reference"});
            }
            args.extend(["image".into(), "tag".into(), target.into(), image.into()]);
        }
        "image-prune" => {
            if !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            args.extend(["image".into(), "prune".into(), "-af".into()]);
        }
        "network-create" | "network-remove" => {
            let value = if operation == "network-create" {
                name
            } else {
                target
            };
            if !valid_docker_object(value) || operation == "network-remove" && !confirmed {
                return json!({"ok":false,"error":"Invalid network name or missing confirmation"});
            }
            args.extend(["network".into()]);
            if operation == "network-create" {
                args.extend(["create".into(), "--driver".into(), "bridge".into()]);
                if request["internal"].as_bool().unwrap_or(false) {
                    args.push("--internal".into());
                }
                args.push(value.into());
            } else {
                args.extend(["rm".into(), value.into()]);
            }
        }
        "volume-create" | "volume-remove" => {
            let value = if operation == "volume-create" {
                name
            } else {
                target
            };
            if !valid_docker_object(value) || operation == "volume-remove" && !confirmed {
                return json!({"ok":false,"error":"Invalid volume name or missing confirmation"});
            }
            args.extend([
                "volume".into(),
                if operation == "volume-create" {
                    "create"
                } else {
                    "rm"
                }
                .into(),
                value.into(),
            ]);
        }
        "volume-prune" => {
            if !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            args.extend(["volume".into(), "prune".into(), "-f".into()]);
        }
        "system-prune" => {
            if !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            args.extend(["system".into(), "prune".into(), "-af".into()]);
        }
        "docker-builder-prune" => {
            if !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            args.extend([
                "builder".into(),
                "prune".into(),
                "--all".into(),
                "--force".into(),
            ]);
        }
        "docker-container-prune" => {
            if !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            args.extend(["container".into(), "prune".into(), "--force".into()]);
        }
        "docker-network-prune" => {
            if !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            args.extend(["network".into(), "prune".into(), "--force".into()]);
        }
        "arg-cache-prune" => return prune_arg_cache(request),
        "server-logs-prune" => return prune_server_logs(request),
        "apt-cache-clean" => return clean_apt_cache(request),
        "compose-up" | "compose-down" | "compose-pull" => {
            if operation == "compose-down" && !confirmed {
                return json!({"ok":false,"error":"Confirmation required"});
            }
            let Some(file) = request["compose_file"].as_str().filter(|file| {
                let path = std::path::Path::new(file);
                path.is_absolute()
                    && !path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                    && path
                        .extension()
                        .and_then(std::ffi::OsStr::to_str)
                        .is_some_and(|extension| ["yml", "yaml"].contains(&extension))
                    && path
                        .metadata()
                        .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 1024 * 1024)
            }) else {
                return json!({"ok":false,"error":"Choose an existing absolute Compose YAML file (maximum 1 MB)"});
            };
            args.extend([
                "compose".into(),
                "-f".into(),
                file.into(),
                match operation {
                    "compose-up" => "up",
                    "compose-down" => "down",
                    _ => "pull",
                }
                .into(),
            ]);
            if operation == "compose-up" {
                args.push("-d".into());
            }
        }
        _ => return json!({"ok":false,"error":"Unsupported Docker operation"}),
    }
    let mut command = std::process::Command::new("docker");
    command.args(&args);
    match command_result(&mut command) {
        Ok((true, output)) => json!({"ok":true,"stdout":output}),
        Ok((false, output)) => json!({"ok":false,"error":output}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

pub(super) fn execute_docker_optimization_status(request: &Value) -> Value {
    let protected = protected_cache_mount_ids(request);
    let docker_df = docker_reclaimable_bytes();
    let retention_days = request["retention_days"]
        .as_u64()
        .filter(|days| [7, 30, 90].contains(days))
        .unwrap_or(30) as u16;
    let log_cutoff = retention_cutoff(retention_days);
    let (journal_bytes, journal_error) = journal_disk_usage();
    let data = json!({
        "measured_at": crate::now(),
        "docker": {
            "available": docker_df.is_ok(),
            "build_cache_bytes": docker_df.as_ref().ok().and_then(|items| items.get("build_cache")).copied().unwrap_or(0),
            "images_bytes": docker_df.as_ref().ok().and_then(|items| items.get("images")).copied().unwrap_or(0),
            "containers_bytes": docker_df.as_ref().ok().and_then(|items| items.get("containers")).copied().unwrap_or(0),
            "volumes_bytes": docker_df.as_ref().ok().and_then(|items| items.get("volumes")).copied().unwrap_or(0),
            "error": docker_df.err().unwrap_or_default()
        },
        "arg_cache": {
            "reclaimable_bytes": arg_cache_reclaimable_bytes(&protected),
            "protected_mounts": protected.len()
        },
        "logs": {
            "journal_bytes": journal_bytes.unwrap_or(0),
            "journal_reclaimable_bytes": old_journal_bytes(log_cutoff),
            "rotated_reclaimable_bytes": old_rotated_log_bytes(log_cutoff),
            "retention_days": retention_days,
            "error": journal_error.unwrap_or_default()
        },
        "apt_cache_bytes": directory_size(std::path::Path::new("/var/cache/apt/archives"))
    });
    json!({"ok":true,"data":data})
}

fn docker_reclaimable_bytes() -> Result<std::collections::HashMap<String, u64>, String> {
    let mut command = std::process::Command::new("docker");
    command.args(["system", "df", "--format", "{{.Type}}\t{{.Reclaimable}}"]);
    let (success, output) = command_result(&mut command)?;
    if !success {
        return Err(output);
    }
    let mut result = std::collections::HashMap::new();
    for line in output.lines() {
        let Some((kind, size)) = line.split_once('\t') else {
            continue;
        };
        let key = match kind.trim() {
            "Build Cache" => "build_cache",
            "Images" => "images",
            "Containers" => "containers",
            "Local Volumes" => "volumes",
            _ => continue,
        };
        if let Some(bytes) = parse_human_bytes(size) {
            result.insert(key.to_owned(), bytes);
        }
    }
    Ok(result)
}

fn parse_human_bytes(value: &str) -> Option<u64> {
    let token = value.trim().split_whitespace().next()?;
    let split = token.find(|character: char| !character.is_ascii_digit() && character != '.')?;
    let amount = token[..split].parse::<f64>().ok()?;
    let unit = token[split..].to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" | "byte" | "bytes" => 1.0,
        "kb" => 1_000.0,
        "kib" => 1_024.0,
        "mb" => 1_000_000.0,
        "mib" => 1_048_576.0,
        "gb" => 1_000_000_000.0,
        "gib" => 1_073_741_824.0,
        "tb" => 1_000_000_000_000.0,
        "tib" => 1_099_511_627_776.0,
        "pb" => 1_000_000_000_000_000.0,
        "pib" => 1_125_899_906_842_624.0,
        _ => return None,
    };
    (amount.is_finite() && amount >= 0.0).then(|| (amount * multiplier).min(u64::MAX as f64) as u64)
}

fn protected_cache_mount_ids(request: &Value) -> std::collections::HashSet<u64> {
    let mut ids = request["protected_cache_mount_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .collect::<std::collections::HashSet<_>>();
    if let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") {
        for line in mountinfo.lines() {
            let Some((left, _)) = line.split_once(" - ") else {
                continue;
            };
            let Some(mount_point) = left.split_whitespace().nth(4) else {
                continue;
            };
            let mount_point = mount_point.replace("\\040", " ");
            if let Some(id) = mount_point
                .strip_prefix("/mnt/arg-s3/mount-")
                .and_then(|value| value.parse().ok())
            {
                ids.insert(id);
            }
        }
    }
    ids
}

fn arg_cache_paths(protected: &std::collections::HashSet<u64>) -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new("/var/cache/arg-s3");
    if !std::fs::symlink_metadata(root).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let id = name.strip_prefix("mount-")?.parse::<u64>().ok()?;
            let kind = entry.file_type().ok()?;
            (!protected.contains(&id) && kind.is_dir()).then(|| entry.path())
        })
        .collect()
}

fn arg_cache_reclaimable_bytes(protected: &std::collections::HashSet<u64>) -> u64 {
    arg_cache_paths(protected)
        .iter()
        .map(|path| directory_size(path))
        .fold(0, u64::saturating_add)
}

fn prune_arg_cache(request: &Value) -> Value {
    if request["confirmed"].as_bool() != Some(true) {
        return json!({"ok":false,"error":"Confirmation required"});
    }
    let protected = protected_cache_mount_ids(request);
    let paths = arg_cache_paths(&protected);
    let mut removed_bytes = 0u64;
    let mut removed_directories = 0usize;
    for path in paths {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.file_type().is_dir() {
            continue;
        }
        removed_bytes = removed_bytes.saturating_add(directory_size(&path));
        if std::fs::remove_dir_all(&path).is_ok() {
            removed_directories += 1;
        }
    }
    json!({"ok":true,"stdout":format!("پاک‌سازی Cacheهای S3 بلااستفادهٔ ARG انجام شد.\nپوشه‌های حذف‌شده: {removed_directories}\nحجم تقریبی آزادشده: {}", format_bytes(removed_bytes))})
}

fn clean_apt_cache(request: &Value) -> Value {
    if request["confirmed"].as_bool() != Some(true) {
        return json!({"ok":false,"error":"Confirmation required"});
    }
    let before = directory_size(std::path::Path::new("/var/cache/apt/archives"));
    let mut command = std::process::Command::new("apt-get");
    command.args(["clean"]);
    match command_result(&mut command) {
        Ok((true, output)) => {
            let freed = before.saturating_sub(directory_size(std::path::Path::new(
                "/var/cache/apt/archives",
            )));
            json!({"ok":true,"stdout":format!("{output}\nپاک‌سازی cache بسته‌های APT انجام شد.\nحجم آزادشده: {}", format_bytes(freed))})
        }
        Ok((false, output)) => json!({"ok":false,"error":output}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}

fn retention_cutoff(days: u16) -> std::time::SystemTime {
    std::time::SystemTime::now() - std::time::Duration::from_secs(u64::from(days) * 86_400)
}

fn journal_disk_usage() -> (Option<u64>, Option<String>) {
    let mut command = std::process::Command::new("journalctl");
    command.args(["--disk-usage"]);
    match command_result(&mut command) {
        Ok((true, output)) => {
            let bytes = output.split_whitespace().find_map(parse_human_bytes);
            (bytes, None)
        }
        Ok((false, output)) => (None, Some(output)),
        Err(error) => (None, Some(error)),
    }
}

fn old_journal_bytes(cutoff: std::time::SystemTime) -> u64 {
    let mut budget = 100_000;
    tree_bytes_matching(
        std::path::Path::new("/var/log/journal"),
        0,
        cutoff,
        &mut budget,
        &|name| name.contains('@') && (name.ends_with(".journal") || name.ends_with(".journal~")),
    )
}

fn old_rotated_log_bytes(cutoff: std::time::SystemTime) -> u64 {
    let mut budget = 100_000;
    tree_bytes_matching(
        std::path::Path::new("/var/log"),
        0,
        cutoff,
        &mut budget,
        &rotation_log_name,
    )
}

fn rotation_log_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let trimmed = lower
        .strip_suffix(".gz")
        .or_else(|| lower.strip_suffix(".xz"))
        .or_else(|| lower.strip_suffix(".zst"))
        .unwrap_or(&lower);
    let numeric_suffix = trimmed.rsplit_once('.').is_some_and(|(_, suffix)| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    });
    let dated_suffix = trimmed.rsplit_once('-').is_some_and(|(_, suffix)| {
        suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_digit())
    });
    let known_log = ["messages", "wtmp", "btmp"]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix));
    (numeric_suffix || dated_suffix) && (trimmed.contains("log") || known_log)
}

fn tree_bytes_matching<F>(
    root: &std::path::Path,
    depth: usize,
    cutoff: std::time::SystemTime,
    budget: &mut usize,
    matches_name: &F,
) -> u64
where
    F: Fn(&str) -> bool,
{
    if depth > 32 || *budget == 0 {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    entries.filter_map(Result::ok).fold(0u64, |total, entry| {
        if *budget == 0 {
            return total;
        }
        *budget -= 1;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if root == std::path::Path::new("/var/log") && name == "journal" {
            return total;
        }
        let Ok(kind) = entry.file_type() else {
            return total;
        };
        let size = if kind.is_dir() {
            tree_bytes_matching(&entry.path(), depth + 1, cutoff, budget, matches_name)
        } else if kind.is_file() && matches_name(&name) {
            entry
                .metadata()
                .ok()
                .and_then(|metadata| {
                    metadata
                        .modified()
                        .ok()
                        .filter(|modified| *modified < cutoff)
                        .map(|_| metadata.len())
                })
                .unwrap_or(0)
        } else {
            0
        };
        total.saturating_add(size)
    })
}

fn prune_rotated_logs(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
    depth: usize,
    budget: &mut usize,
) -> Result<u64, String> {
    if depth > 32 || *budget == 0 {
        return Ok(0);
    }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.to_string()),
    };
    let mut removed = 0u64;
    for entry in entries {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if root == std::path::Path::new("/var/log") && name == "journal" {
            continue;
        }
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_dir() {
            removed = removed.saturating_add(prune_rotated_logs(
                &entry.path(),
                cutoff,
                depth + 1,
                budget,
            )?);
        } else if kind.is_file() && rotation_log_name(&name) {
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            if metadata.modified().is_ok_and(|modified| modified < cutoff) {
                let size = metadata.len();
                if std::fs::remove_file(entry.path()).is_ok() {
                    removed = removed.saturating_add(size);
                }
            }
        }
    }
    Ok(removed)
}

fn prune_server_logs(request: &Value) -> Value {
    if request["confirmed"].as_bool() != Some(true) {
        return json!({"ok":false,"error":"Confirmation required"});
    }
    let days = request["retention_days"]
        .as_u64()
        .filter(|days| [7, 30, 90].contains(days))
        .unwrap_or(30) as u16;
    if !std::fs::symlink_metadata("/var/log").is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return json!({"ok":false,"error":"مسیر امن /var/log در دسترس نیست."});
    }
    let mut journal = std::process::Command::new("journalctl");
    journal.args(["--rotate", &format!("--vacuum-time={days}d")]);
    let journal_output = command_result(&mut journal);
    let mut budget = 100_000;
    let rotated = match prune_rotated_logs(
        std::path::Path::new("/var/log"),
        retention_cutoff(days),
        0,
        &mut budget,
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            return json!({"ok":false,"error":format!("پاک‌سازی فایل‌های قدیمی لاگ ناموفق بود: {error}")})
        }
    };
    let journal_text = match journal_output {
        Ok((true, output)) => output,
        Ok((false, output)) => format!("journalctl: {output}"),
        Err(error) => format!("journalctl: {error}"),
    };
    json!({"ok":true,"stdout":format!("{journal_text}\nدورهٔ نگه‌داری: {days} روز\nحجم فایل‌های چرخشی حذف‌شده: {}\nJournalهای فعال نگه داشته شدند.", format_bytes(rotated))})
}

fn directory_size(path: &std::path::Path) -> u64 {
    tree_size(path, 0, &mut 100_000)
}

fn tree_size(path: &std::path::Path, depth: usize, budget: &mut usize) -> u64 {
    if depth > 32 || *budget == 0 {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Ok(entry) = entry else { continue };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let size = if kind.is_dir() {
            tree_size(&entry.path(), depth + 1, budget)
        } else if kind.is_file() {
            entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
        } else {
            0
        };
        total = total.saturating_add(size);
    }
    total
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1_000.0 && unit < UNITS.len() - 1 {
        value /= 1_000.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}

fn application_container_ids(app_id: &str) -> Result<Vec<String>, String> {
    if !apps::valid_uuid(app_id) {
        return Err("شناسهٔ اپلیکیشن Docker معتبر نیست.".into());
    }
    let label = format!("label=com.arg.application_id={app_id}");
    let output = match std::process::Command::new("docker")
        .args(["ps", "-aq", "--filter", &label])
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("بررسی کانتینرهای اپ ناموفق بود: {error}")),
    };
    if !output.status.success() {
        return Err(format!(
            "Docker نتوانست کانتینرهای این اپ را فهرست کند: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let ids = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if ids.len() > 256
        || ids.iter().any(|id| {
            !(12..=64).contains(&id.len()) || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err("فهرست شناسه‌های کانتینر Docker خارج از محدودهٔ امن است.".into());
    }
    Ok(ids)
}

pub(super) fn preflight_application_containers(app_id: &str) -> Result<(), String> {
    let _ = application_container_ids(app_id)?;
    Ok(())
}

pub(super) fn purge_application_containers(app_id: &str) -> Result<Vec<String>, String> {
    let ids = application_container_ids(app_id)?;
    if ids.is_empty() {
        return Ok(ids);
    }
    let output = std::process::Command::new("docker")
        .arg("rm")
        .arg("-f")
        .args(&ids)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("حذف کانتینرهای متعلق به اپ ناموفق بود: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Docker نتوانست کانتینرهای اپ را حذف کند: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::{parse_human_bytes, rotation_log_name};

    #[test]
    fn docker_human_sizes_are_parsed_in_decimal_and_binary_units() {
        assert_eq!(parse_human_bytes("1.5GB (25%)"), Some(1_500_000_000));
        assert_eq!(parse_human_bytes("2GiB"), Some(2_147_483_648));
        assert_eq!(parse_human_bytes("0B (0%)"), Some(0));
        assert_eq!(parse_human_bytes("unknown"), None);
    }

    #[test]
    fn only_rotated_log_names_are_selected_for_old_log_cleanup() {
        assert!(rotation_log_name("syslog.2.gz"));
        assert!(rotation_log_name("access.log.1"));
        assert!(!rotation_log_name("syslog"));
        assert!(!rotation_log_name("database.dump.1.gz"));
    }
}
