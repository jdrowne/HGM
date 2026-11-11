use sha2::{Digest, Sha256};

pub const LANGUAGES: [&str; 6] = ["python", "rust", "go", "javascript", "cpp", "java"];

const NPM_TEST_COMMANDS: [&str; 5] = [
    "set -e",
    "[ ! -e node_modules ] && ln -s /npm-install/node_modules .",
    "[ ! -e package-lock.json ] && ln -s /npm-install/package-lock.json .",
    r"sed -i 's/\bxtest(/test(/g' *.spec.js",
    "npm run test",
];

const CPP_TEST_COMMANDS: [&str; 6] = [
    "set -e",
    "[ ! -d \"build\" ] && mkdir build",
    "cd build",
    "cmake -DEXERCISM_RUN_ALL_TESTS=1 -G \"Unix Makefiles\" ..",
    "make",
    "cd ../",
];

const PIP_PACKAGES: [&str; 11] = [
    "flake8~=5.0.4",
    "pylint~=2.17.1",
    "black==22.3.0",
    "yapf~=0.32.0",
    "tomli>=1.1.0",
    "pytest",
    "pytest-env",
    "pytest-asyncio",
    "async_timeout",
    "trio",
    "testpath",
];

pub fn test_commands(language: &str) -> &'static [&'static str] {
    match language {
        "python" => &["pytest -rA --tb=long"],
        "rust" => &["cargo test -- --include-ignored"],
        "go" => &["go test ./..."],
        "javascript" => &NPM_TEST_COMMANDS,
        "cpp" => &CPP_TEST_COMMANDS,
        "java" => &["./gradlew test"],
        _ => &[],
    }
}

pub fn test_cmd(language: &str) -> String {
    let separator = if matches!(language, "javascript" | "cpp") {
        "\n"
    } else {
        " "
    };
    test_commands(language).join(separator)
}

fn script<S: AsRef<str>>(header: &str, lines: &[S]) -> String {
    let mut out = format!("#!/bin/bash\n{header}\n");
    for line in lines {
        out.push_str(line.as_ref());
        out.push('\n');
    }
    out
}

pub fn get_eval_script<S: AsRef<str>>(commands: &[S]) -> String {
    script("set -uxo pipefail", commands)
}

const DOCKERFILE_BASE: &str = r#"
FROM buildpack-deps:jammy

# Install Python 3.11
RUN apt-get update && apt-get install -y \
    software-properties-common \
    cmake \
    libboost-all-dev \
    && add-apt-repository ppa:deadsnakes/ppa \
    && apt-get update \
    && apt-get install -y \
    python3.11 \
    python3.11-venv \
    python3.11-dev \
    python3-pip \
    ca-certificates-java \
    openjdk-21-jdk \
    libtbb-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Go with architecture detection
RUN ARCH=$(uname -m) && \
    if [ "$ARCH" = "x86_64" ]; then \
        GOARCH="amd64"; \
    elif [ "$ARCH" = "aarch64" ]; then \
        GOARCH="arm64"; \
    else \
        false; \
    fi && \
    curl -L "https://golang.org/dl/go1.21.5.linux-$GOARCH.tar.gz" -o go.tar.gz && \
    tar -C /usr/local -xzf go.tar.gz && \
    rm go.tar.gz
ENV PATH="/usr/local/go/bin:${PATH}"

# Install Rust
ADD https://sh.rustup.rs /tmp/rustup.sh
RUN chmod +x /tmp/rustup.sh && /tmp/rustup.sh -y && rm /tmp/rustup.sh
ENV PATH="/root/.cargo/bin:${PATH}"

# Install Node.js and dependencies
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir -p /npm-install && \
    cd /npm-install && \
    npm init -y && \
    npm install \
    jest \
    @babel/core@7.25.2 \
    @exercism/babel-preset-javascript@0.2.1 \
    @exercism/eslint-config-javascript@0.6.0 \
    @types/jest@29.5.12 \
    @types/node@20.12.12 \
    babel-jest@29.6.4 \
    core-js@3.37.1 \
    eslint@8.49.0

# Download and install conda
RUN wget 'https://repo.anaconda.com/miniconda/Miniconda3-py311_23.11.0-2-Linux-{conda_arch}.sh' -O miniconda.sh \
    && bash miniconda.sh -b -p /opt/miniconda3
# Add conda to PATH
ENV PATH=/opt/miniconda3/bin:$PATH
# Add conda to shell startup scripts like .bashrc (DO NOT REMOVE THIS)
RUN conda init --all
RUN conda config --append channels conda-forge

RUN adduser --disabled-password --gecos 'dog' nonroot
"#;

const DOCKERFILE_ENV: &str = r#"FROM --platform={platform} {base_image}

COPY ./setup_env.sh /root/
RUN chmod +x /root/setup_env.sh
RUN /bin/bash -c "source ~/.bashrc && /root/setup_env.sh"

WORKDIR /testbed/

# Automatically activate the testbed environment
RUN echo "source /opt/miniconda3/etc/profile.d/conda.sh && conda activate testbed" > /root/.bashrc
"#;

const DOCKERFILE_INSTANCE: &str = r"FROM --platform={platform} {env_image_name}

COPY ./setup_repo.sh /root/
COPY ./polyglot /polyglot
RUN /bin/bash /root/setup_repo.sh

WORKDIR /testbed/
";

pub const NAMESPACE: &str = "pb";

pub fn host_arch() -> &'static str {
    arch_for(std::env::consts::ARCH)
}

pub fn arch_for(machine: &str) -> &'static str {
    if matches!(machine, "aarch64" | "arm64") {
        "arm64"
    } else {
        "x86_64"
    }
}

fn py_list_repr(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", inner.join(", "))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestSpec {
    pub instance_id: String,
    pub repo: String,
    pub repo_script_list: Vec<String>,
    pub eval_script_list: Vec<String>,
    pub env_script_list: Vec<String>,
    pub arch: String,
    pub namespace: String,
}

impl TestSpec {
    pub fn setup_env_script(&self) -> String {
        script("set -euxo pipefail", &self.env_script_list)
    }

    pub fn eval_script(&self) -> String {
        script("set -uxo pipefail", &self.eval_script_list)
    }

    pub fn install_repo_script(&self) -> String {
        script("set -euxo pipefail", &self.repo_script_list)
    }

    pub fn base_image_key(&self) -> String {
        format!("{}.base.{}:latest", self.namespace, self.arch)
    }

    pub fn env_image_key(&self) -> String {
        let digest = Sha256::digest(py_list_repr(&self.env_script_list).as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("{}.env.{}.{}:latest", self.namespace, self.arch, &hex[..22])
    }

    pub fn instance_image_key(&self) -> String {
        format!(
            "{}.eval.{}.{}:latest",
            self.namespace, self.arch, self.instance_id
        )
    }

    pub fn container_name(&self, run_id: &str) -> String {
        if run_id.is_empty() {
            format!("{}.eval.{}", self.namespace, self.instance_id)
        } else {
            format!("{}.eval.{}.{run_id}", self.namespace, self.instance_id)
        }
    }

    pub fn platform(&self) -> &'static str {
        if self.arch == "arm64" {
            "linux/arm64/v8"
        } else {
            "linux/x86_64"
        }
    }

    pub fn base_dockerfile(&self) -> String {
        let conda_arch = if self.arch == "arm64" {
            "aarch64"
        } else {
            &self.arch
        };
        DOCKERFILE_BASE.replace("{conda_arch}", conda_arch)
    }

    pub fn env_dockerfile(&self) -> String {
        DOCKERFILE_ENV
            .replace("{platform}", self.platform())
            .replace("{base_image}", &self.base_image_key())
    }

    pub fn instance_dockerfile(&self) -> String {
        DOCKERFILE_INSTANCE
            .replace("{platform}", self.platform())
            .replace("{env_image_name}", &self.env_image_key())
    }
}

fn modified_files(test_patch: &str) -> Vec<&str> {
    test_patch
        .split('\n')
        .filter_map(|line| line.find("--- a/").map(|i| &line[i + 6..]))
        .collect()
}

fn added_files(test_patch: &str) -> Vec<&str> {
    let lines: Vec<&str> = test_patch.split('\n').collect();
    lines
        .windows(2)
        .filter(|pair| pair[1].starts_with("new file mode"))
        .filter_map(|pair| {
            let rest = pair[0].strip_prefix("diff --git a/")?;
            let (a, b) = rest.split_once(" b/")?;
            (a == b && !a.is_empty() && !a.contains(char::is_whitespace)).then_some(a)
        })
        .collect()
}

fn eval_script_list(language: &str, base_commit: &str, test_patch: &str) -> Vec<String> {
    let added = added_files(test_patch);
    let modified: Vec<&str> = modified_files(test_patch)
        .into_iter()
        .filter(|f| !added.contains(f))
        .collect();
    let mut reset = Vec::new();
    if !modified.is_empty() {
        reset.push(format!(
            "git checkout {base_commit} -- {}",
            modified.join(" ")
        ));
    }
    if !added.is_empty() {
        reset.push(format!("rm -f {}", added.join(" ")));
    }
    if language == "cpp" {
        reset.push("rm -rf build/".to_owned());
    }
    let reset = if reset.is_empty() {
        "true".to_owned()
    } else {
        reset.join(" && ")
    };
    vec![
        "source /opt/miniconda3/bin/activate".to_owned(),
        "conda activate testbed".to_owned(),
        "cd /testbed".to_owned(),
        "git config --global --add safe.directory /testbed".to_owned(),
        "cd /testbed".to_owned(),
        "git status".to_owned(),
        "git show".to_owned(),
        format!("git diff {base_commit}"),
        "source /opt/miniconda3/bin/activate".to_owned(),
        "conda activate testbed".to_owned(),
        reset.clone(),
        format!("git apply -v - <<'EOF_114329324912'\n{test_patch}\nEOF_114329324912"),
        test_cmd(language),
        reset,
    ]
}

fn env_script_list(language: &str) -> Vec<String> {
    let mut list = vec![
        "source /opt/miniconda3/bin/activate".to_owned(),
        "conda create -n testbed python=3.11  -y".to_owned(),
        "conda activate testbed".to_owned(),
    ];
    if language == "python" {
        list.push(format!("python -m pip install {}", PIP_PACKAGES.join(" ")));
    }
    list
}

fn repo_script_list(repo: &str, base_commit: &str) -> Vec<String> {
    vec![
        format!("cd /{repo}"),
        "git clone . /testbed".to_owned(),
        "chmod -R 777 /testbed".to_owned(),
        "cd /testbed".to_owned(),
        format!("git reset --hard {base_commit}"),
        "source /opt/miniconda3/bin/activate".to_owned(),
        "conda activate testbed".to_owned(),
        "echo \"Current environment: $CONDA_DEFAULT_ENV\"".to_owned(),
    ]
}

pub fn make_test_spec(instance: &serde_json::Value, machine: &str) -> TestSpec {
    let field = |key: &str| instance[key].as_str().unwrap_or_default();
    let language = field("language");
    let base_commit = field("base_commit");
    TestSpec {
        instance_id: field("instance_id").to_owned(),
        repo: field("repo").to_owned(),
        repo_script_list: repo_script_list(field("repo"), base_commit),
        eval_script_list: eval_script_list(language, base_commit, field("test_patch")),
        env_script_list: env_script_list(language),
        arch: arch_for(machine).to_owned(),
        namespace: NAMESPACE.to_owned(),
    }
}
