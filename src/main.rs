use anyhow::Result;
use anyhow::bail;
use glob::glob;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

// Dump out all nvme devices seen, their devfs path, and their raw character
// device path

fn main() -> Result<()> {
    // Cache the /dev/rdsk/ links
    let rdsk_paths = {
        let cwd = std::env::current_dir()?;
        std::env::set_current_dir("/dev/rdsk")?;

        let mut rdsk_paths: HashMap<String, String> = HashMap::new();
        for entry in glob("/dev/rdsk/*")? {
            let entry = entry?;
            let link = std::fs::read_link(&entry)?;
            if format!("{link:?}").contains(":wd") {
                match std::fs::canonicalize(link) {
                    Ok(link) => {
                        rdsk_paths.insert(
                            link.into_os_string().into_string().unwrap(),
                            entry.into_os_string().into_string().unwrap(),
                        );
                    }

                    Err(_) => {
                        println!("{entry:?} => ?");
                    }
                }
            }
        }

        std::env::set_current_dir(cwd)?;
        rdsk_paths
    };

    // Find all nvme to create an instance map
    let cmd = Command::new("pfexec")
        .arg("nvmeadm")
        .arg("list")
        .arg("-p")
        .arg("-o")
        .arg("model,serial,instance")
        .output()?;

    let text = String::from_utf8_lossy(&cmd.stdout);

    let mut instance_map: HashMap<&str, (&str, &str)> = HashMap::new();

    for line in text.split("\n") {
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split(':').collect();
        let model = parts[0];
        let serial = parts[1];
        let instance = parts[2];

        instance_map.insert(instance, (model, serial));
    }

    // Walk all "nvme" nodes, then find the underlying "blkdev", then use that
    // devfs path.

    let mut di = devinfo::DevInfo::new()?;

    let mut w = di.walk_driver("nvme");
    while let Some(n) = w.next().transpose()? {
        let instance = format!("{}{}", n.driver_name().unwrap(), n.instance().unwrap(),);

        let Some((model, serial)) = instance_map.get(instance.as_str()) else {
            continue;
        };
        let devfs_path = format!("/devices{}", n.devfs_path()?);

        println!(
            "> found {} / {} / {} / {}",
            instance, model, serial, devfs_path,
        );
        let mut bdi = devinfo::DevInfo::new_path(n.devfs_path()?)?;
        let mut bw = bdi.walk_driver("blkdev");
        let mut times = 0;

        while let Some(bn) = bw.next().transpose()? {
            if times != 0 {
                bail!("multiple blkdev?!");
            }

            let devfs_path = format!("/devices{}", bn.devfs_path()?);

            println!(
                ">> {}{}: {}",
                bn.driver_name().unwrap(),
                bn.instance().unwrap(),
                devfs_path,
            );

            let raw = format!("{devfs_path}:wd,raw");

            println!(">>> raw char device: {raw}");

            if !Path::exists(Path::new(&raw)) {
                bail!("{raw} does not exist! needs gpt, run zpool create");
            }

            println!(">>> rdsk path: {:?}", rdsk_paths.get(&raw));

            times += 1;
        }
    }

    Ok(())
}
