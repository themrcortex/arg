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
    let result = privileged_request_async(json!({"action":"docker-operation","operation":input.action,"target":input.target,"name":input.name,"image":input.image,"ports":input.ports,"env":input.env,"network":input.network,"restart":input.restart,"compose_file":input.compose_file,"context_path":input.context_path,"command":input.command,"confirmed":input.confirmed})).await?;
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
            "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}\t{{.State}}",
        ],
        &["id", "names", "image", "status", "ports", "state"],
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
        if let Some(rows) = containers.as_mut().ok() {
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
    let networks = docker_table(
        &[
            "network",
            "ls",
            "--no-trunc",
            "--format",
            "{{.ID}}\t{{.Name}}\t{{.Driver}}\t{{.Scope}}",
        ],
        &["id", "name", "driver", "scope"],
    );
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
            if operation == "enable" || operation == "disable" {
                args.extend([
                    "systemctl".to_owned(),
                    operation.to_owned(),
                    "docker".to_owned(),
                ]);
            } else {
                args.extend([
                    "systemctl".to_owned(),
                    operation.to_owned(),
                    "docker".to_owned(),
                ]);
            }
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
            for port in ports {
                args.extend(["-p".into(), port.into()]);
            }
            for value in envs {
                args.extend(["--env".into(), value.into()]);
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
            args.extend([
                "network".into(),
                if operation == "network-create" {
                    "create"
                } else {
                    "rm"
                }
                .into(),
                value.into(),
            ]);
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
