use std::process::Command;
use std::collections::HashMap;
use anyhow::bail;
use anyhow::Result;
use std::path::Path;

// Dump out all nvme devices seen, their devfs path, and their raw character
// device path

fn main() -> Result<()> {
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
        if line.len() == 0 {
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

            if !Path::exists(&Path::new(&raw)) {
                bail!("{raw} does not exist!");
            }

            times += 1;
        }
    }

    Ok(())
}
