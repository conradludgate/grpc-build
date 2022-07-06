use anyhow::{anyhow, Context, Ok, Result};
use prost::Message;
use prost_build::{protoc, protoc_include, Module};
use prost_types::{FileDescriptorProto, FileDescriptorSet};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::Path,
    process::Command,
};

pub mod base;
mod builder;
pub mod tree;
pub use builder::Builder;

impl Builder {
    pub fn build(
        self,
        in_dir: impl AsRef<Path>,
        out_dir: impl AsRef<Path>,
        force: bool,
    ) -> Result<(), anyhow::Error> {
        if !force && out_dir.as_ref().exists() {
            return Err(anyhow!(
                "the output directory already exists: {}",
                out_dir.as_ref().display()
            ));
        }

        base::prepare_out_dir(out_dir.as_ref()).context("failed to prepare out dir")?;

        let impls = self
            .compile(in_dir.as_ref(), out_dir.as_ref())
            .context("failed to compile the protos")?;

        base::refactor(out_dir.as_ref()).context("failed to refactor the protos")?;

        std::fs::File::options()
            .write(true)
            .append(true)
            .open(out_dir.as_ref().join("mod.rs"))?
            .write_all(impls.as_bytes())?;

        Ok(())
    }

    fn compile(mut self, input_dir: &Path, out_dir: &Path) -> Result<String, anyhow::Error> {
        let tmp = tempfile::Builder::new().prefix("grpc-build").tempdir()?;
        let file_descriptor_path = tmp.path().join("grpc-descriptor-set");

        self.run_protoc(input_dir.as_ref(), &file_descriptor_path)?;

        let buf = std::fs::read(&file_descriptor_path)?;
        let file_descriptor_set =
            FileDescriptorSet::decode(&*buf).context("invalid FileDescriptorSet")?;

        let impls = self.generate_impls(&file_descriptor_set);

        self.generate_services(out_dir, file_descriptor_set)?;

        Ok(impls)
    }

    fn run_protoc(
        &self,
        input_dir: &Path,
        file_descriptor_path: &Path,
    ) -> Result<(), anyhow::Error> {
        let protos = crate::base::get_protos(input_dir).collect::<Vec<_>>();

        let compile_includes: &Path = match input_dir.parent() {
            None => Path::new("."),
            Some(parent) => parent,
        };

        let mut cmd = Command::new(protoc());
        cmd.arg("--include_imports")
            .arg("--include_source_info")
            .arg("-o")
            .arg(file_descriptor_path);
        cmd.arg("-I").arg(compile_includes);

        cmd.arg("-I").arg(protoc_include());

        for arg in &self.protoc_args {
            cmd.arg(arg);
        }

        for proto in &protos {
            cmd.arg(proto);
        }

        cmd.output().context(
            "failed to invoke protoc (hint: https://docs.rs/prost-build/#sourcing-protoc)",
        )?;
        Ok(())
    }

    fn generate_impls(&mut self, file_descriptor_set: &FileDescriptorSet) -> String {
        if self.prost_types {
            add_prost_types(&mut self.extern_paths);
        }

        file_descriptor_set
            .file
            .iter()
            .flat_map(derive_named_messages)
            .filter_map(|(name, derive)| self.include_ident(&name).then(|| derive))
            .collect()
    }

    fn generate_services(
        mut self,
        out_dir: &Path,
        file_descriptor_set: FileDescriptorSet,
    ) -> Result<(), anyhow::Error> {
        let service_generator = self.tonic.service_generator();
        self.prost.service_generator(service_generator);

        let requests = file_descriptor_set
            .file
            .into_iter()
            .map(|descriptor| {
                (
                    Module::from_protobuf_package_name(descriptor.package()),
                    descriptor,
                )
            })
            .collect::<Vec<_>>();

        let file_names = requests
            .iter()
            .map(|(module, _)| (module.clone(), module.to_file_name_or("_")))
            .collect::<HashMap<Module, String>>();

        let modules = self.prost.generate(requests)?;
        for (module, content) in &modules {
            let file_name = file_names
                .get(module)
                .expect("every module should have a filename");
            let output_path = out_dir.join(file_name);

            let previous_content = std::fs::read(&output_path);

            // only write the file if the contents have changed
            if previous_content
                .map(|previous_content| previous_content != content.as_bytes())
                .unwrap_or(true)
            {
                std::fs::write(output_path, content)?;
            }
        }

        Ok(())
    }

    pub fn include_ident(&self, pb_ident: &str) -> bool {
        if self.extern_paths.contains(pb_ident) {
            return false;
        }

        for (idx, _) in pb_ident.rmatch_indices('.') {
            if self.extern_paths.contains(&pb_ident[..idx]) {
                return false;
            }
        }

        true
    }
}

/// Build annotations for the top-level messages in a file,
fn derive_named_messages(
    descriptor: &FileDescriptorProto,
) -> impl Iterator<Item = (String, String)> + '_ {
    let namespace = descriptor.package();
    descriptor.message_type.iter().map(|message| {
        let fq_name = fully_qualified_name(namespace, message.name());
        let message_name = fq_name.trim_start_matches('.');
        let fq_type_path = fully_qualified_type_path(namespace, message.name());
        let type_path = fq_type_path.trim_start_matches("::");
        let derive = format!(
            "
impl {type_path} {{
    pub fn message_name() -> &'static str {{
        {message_name:?}
    }}
}}"
        );
        (fq_name, derive)
    })
}

fn fully_qualified_name(namespace: &str, name: &str) -> String {
    let prefix = if namespace.is_empty() { "" } else { "." };
    format!("{prefix}{namespace}.{name}")
}

fn fully_qualified_type_path(namespace: &str, name: &str) -> String {
    let prefix = if namespace.is_empty() { "" } else { "::" };
    let namespace = namespace.replace('.', "::");
    let name = to_upper_camel(name);
    format!("{prefix}{namespace}::{name}")
}

fn add_prost_types(extern_paths: &mut HashSet<String>) {
    extern_paths.insert(".google.protobuf".to_string());
    extern_paths.insert(".google.protobuf.BoolValue".to_string());
    extern_paths.insert(".google.protobuf.BytesValue".to_string());
    extern_paths.insert(".google.protobuf.DoubleValue".to_string());
    extern_paths.insert(".google.protobuf.Empty".to_string());
    extern_paths.insert(".google.protobuf.FloatValue".to_string());
    extern_paths.insert(".google.protobuf.Int32Value".to_string());
    extern_paths.insert(".google.protobuf.Int64Value".to_string());
    extern_paths.insert(".google.protobuf.StringValue".to_string());
    extern_paths.insert(".google.protobuf.UInt32Value".to_string());
    extern_paths.insert(".google.protobuf.UInt64Value".to_string());
}

/// Converts a `snake_case` identifier to an `UpperCamel` case Rust type identifier.
pub fn to_upper_camel(s: &str) -> String {
    use heck::ToUpperCamelCase;
    let mut ident = s.to_upper_camel_case();

    // Suffix an underscore for the `Self` Rust keyword as it is not allowed as raw identifier.
    if ident == "Self" {
        ident += "_";
    }
    ident
}
