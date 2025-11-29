use std::path::{Path, PathBuf};

use anyhow::Result;
use libcontainer::oci_spec::runtime::{
    LinuxBuilder, LinuxIdMappingBuilder, LinuxNamespace, LinuxNamespaceBuilder,
    LinuxNamespaceType, Mount, ProcessBuilder, RootBuilder, Spec,
};

pub struct SysUserParms {
    pub uid: u32,
    pub gid: u32,
}

pub fn get_spec(sys_user: &SysUserParms) -> Result<Spec> {
    let namespaces = build_rootless_namespaces()?;
    let linux = build_linux_config(sys_user, namespaces)?;
    let mounts = build_rootless_mounts();
    let process = build_process()?;
    let root = build_root()?;

    let mut spec = Spec::default();
    spec.set_process(Some(process))
        .set_root(Some(root))
        .set_linux(Some(linux))
        .set_mounts(Some(mounts));

    Ok(spec)
}

fn build_rootless_namespaces() -> Result<Vec<LinuxNamespace>> {
    let mut namespaces = filter_default_namespaces();
    namespaces.push(create_user_namespace()?);
    Ok(namespaces)
}

fn filter_default_namespaces() -> Vec<LinuxNamespace> {
    libcontainer::oci_spec::runtime::get_default_namespaces()
        .into_iter()
        .filter(|ns| !is_excluded_namespace(ns))
        .collect()
}

fn is_excluded_namespace(ns: &LinuxNamespace) -> bool {
    matches!(
        ns.typ(),
        LinuxNamespaceType::Network | LinuxNamespaceType::User
    )
}

fn create_user_namespace() -> Result<LinuxNamespace> {
    LinuxNamespaceBuilder::default()
        .typ(LinuxNamespaceType::User)
        .build()
        .map_err(Into::into)
}

fn build_linux_config(
    sys_user: &SysUserParms,
    namespaces: Vec<LinuxNamespace>,
) -> Result<libcontainer::oci_spec::runtime::Linux> {
    LinuxBuilder::default()
        .namespaces(namespaces)
        .uid_mappings(vec![create_id_mapping(sys_user.uid)?])
        .gid_mappings(vec![create_id_mapping(sys_user.gid)?])
        .build()
        .map_err(Into::into)
}

fn create_id_mapping(host_id: u32) -> Result<libcontainer::oci_spec::runtime::LinuxIdMapping> {
    LinuxIdMappingBuilder::default()
        .host_id(host_id)
        .container_id(0_u32)
        .size(1_u32)
        .build()
        .map_err(Into::into)
}

fn build_rootless_mounts() -> Vec<Mount> {
    libcontainer::oci_spec::runtime::get_default_mounts()
        .into_iter()
        .map(|mut mount| {
            if is_sys_mount(&mount) {
                configure_sys_mount(&mut mount);
            } else {
                filter_mount_options(&mut mount);
            }
            mount
        })
        .collect()
}

fn is_sys_mount(mount: &Mount) -> bool {
    mount.destination().eq(Path::new("/sys"))
}

fn configure_sys_mount(mount: &mut Mount) {
    mount
        .set_source(Some(PathBuf::from("/sys")))
        .set_typ(Some(String::from("none")))
        .set_options(Some(vec![
            "rbind".to_string(),
            "nosuid".to_string(),
            "noexec".to_string(),
            "nodev".to_string(),
            "ro".to_string(),
        ]));
}

fn filter_mount_options(mount: &mut Mount) {
    let filtered_options: Vec<String> = mount
        .options()
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .filter(|opt| !opt.starts_with("gid=") && !opt.starts_with("uid="))
        .map(|opt| opt.to_string())
        .collect();

    mount.set_options(Some(filtered_options));
}

fn build_process() -> Result<libcontainer::oci_spec::runtime::Process> {
    ProcessBuilder::default()
        .args(vec!["/app/bootstrap".to_string()])
        .build()
        .map_err(Into::into)
}

fn build_root() -> Result<libcontainer::oci_spec::runtime::Root> {
    RootBuilder::default()
        .readonly(false)
        .build()
        .map_err(Into::into)
}
