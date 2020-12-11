use nix::unistd;
use std::env::set_var; // For exporting environmen vars.
use std::ffi::CString;
use std::fs; // For getting fuctions for dir creation.
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path; // For working with file existences.
use std::process::{exit, Command};

/*
use sys_mount::{Mount, MountFlags, SupportedFilesystems};
use execute::Execute; // For simplifying external command execution.
pub fn bind_mount(source_file: &str, target_file: &str) {
    // Fetch a list of supported file systems.
    // When mounting, a file system will be selected from this.
    let supported = SupportedFilesystems::new().unwrap();

    // Attempt to mount the src device to the dest directory.
    let mount_result = Mount::new(source_file, target_file, &supported, MountFlags::BIND, None);

    match mount_result {
        Ok(_mount) => {
            // Make the mount temporary, so that it will be unmounted on drop.
            // let mount = _mount.into_unmount_drop(UnmountFlags::DETACH);
        }
        Err(why) => {
            eprintln!("Error: Failed to mount device: {}", why);
            exit(1);
        }
    }
}



pub fn run_externc(
    executable: &str,
    exe_arg1: &str,
    exe_arg2: &str,
    exe_arg3: &str,
    err_msg: &str,
) {
    let mut command = Command::new(executable);

    // Noob way XD
    command.arg(exe_arg1);
    command.arg(exe_arg2);
    if exe_arg3 != "" {
        command.arg(exe_arg3);
    }

    if let Some(exit_code) = command.execute().unwrap() {
        if exit_code != 0 {
            eprintln!("{}", err_msg);
            exit(exit_code);
        }
    }
}
*/

pub fn executev(args: &[&str]) {
    let args: Vec<CString> = args
        .iter()
        .map(|t| CString::new(*t).expect("not a proper CString"))
        .collect();
    unistd::execv(&args[0], &args).expect("failed");
}

pub fn chmod(file: &str, mode: u32) {
    fs::set_permissions(file, fs::Permissions::from_mode(mode)).unwrap();
}

pub fn extract_file(extern_file: &str, intern_file: &'static [u8], extern_mode: u32) {
    match fs::write(extern_file, intern_file) {
        Ok(_) => {
            chmod(extern_file, extern_mode);
        }
        Err(why) => {
            eprintln!("Error: Failed to write {} file: {}", extern_file, why);
            exit(1);
        }
    }
}

pub fn mount(my_args: &[&str]) {
    let mount_prog = "/dev/mount";

    if !Path::new(mount_prog).exists() {
        extract_file(mount_prog, include_bytes!("asset/mount"), 0o777)
    }

    Command::new(mount_prog)
        .args(my_args)
        .spawn()
        .expect("Error: ");
}

pub fn job() {
    // Export some possibly required env vars for magisk
    set_var("FIRST_STAGE", "1");
    set_var("ASH_STANDALONE", "1");

    // Initialize vars
    let init_real = "/init.real";
    let bin_dir = if Path::new("/sbin").exists() {
        "/sbin"
    } else {
        "/system/bin"
    };

    let superuser_config = "/init.superuser.rc";
    let magisk_config = &format!("{}{}", bin_dir, "/.magisk/config");

    let magisk_apk_dir = "/system/priv-app/MagiskSu";
    let magisk_bin = &format!("{}{}", bin_dir, "/magisk");

    let _magisk_bin_data_x86 = include_bytes!("asset/magisk");
    let _magisk_bin_data_x64 = include_bytes!("asset/magisk64");
    let magisk_bin_data: &'static [u8] = if Path::new("/system/lib64").exists() {
        _magisk_bin_data_x64
    } else {
        _magisk_bin_data_x86
    };

    //// Initialize bin_dir
    if bin_dir != "/sbin" {
        for dir in ["/dev/magisk/upper", "/dev/magisk/work"].iter() {
            fs::create_dir_all(dir).expect("Error: Failed to setup bin_dir at /dev");
            extract_file("/dev/magisk_bin", magisk_bin_data, 0o755);
            Command::new("/dev/magisk_bin").args(&[
                "--clone-attr",
                "/system/bin",
                "/dev/magisk/upper",
            ]);
        }
    } else {
        // Remount required mountpoints as rw
        mount(&[&"-o", "remount,rw", "/"]);
        if Path::new(bin_dir).exists() {
            mount(&[&"-o", "remount,rw", bin_dir]);
        }
    }

    // Create required dirs in bin_dir
    let mirror_dir = [
        format!("{}{}", bin_dir, "/.magisk/mirror/data"),
        format!("{}{}", bin_dir, "/.magisk/mirror/system"),
        format!("{}{}", bin_dir, "/.magisk/modules"),
        // format!("{}{}", bin_dir, "/.magisk/block"),
    ];

    for dir in mirror_dir.iter() {
        fs::create_dir_all(dir).expect(&format!("Failed to create {} dir", dir));
    }

    //// Bind data and system mirrors in bin_dir
    let mut mirror_count = 1;
    for mirror_source in ["/data", "/system"].iter() {
        mount(&[&"-o", "bind", mirror_source, &mirror_dir[mirror_count]]);
        mirror_count += 1;
    }

    // Double remount bin_dir
    mount(&[&"-o", "remount,rw", bin_dir]);

    ///////////////////////////
    //// Initialize magisk ////
    // Extract magisk and set it up

    extract_file(superuser_config, include_bytes!("config/su"), 0o755);
    extract_file(magisk_config, include_bytes!("config/magisk"), 0o755);
    extract_file(magisk_bin, magisk_bin_data, 0o755);

    // Link magisk applets
    for file in ["su", "resetprop", "magiskhide"].iter() {
        if !Path::new(&format!("{}/{}", bin_dir, file)).exists() {
            match symlink(magisk_bin, format!("{}/{}", bin_dir, file)) {
                Ok(_) => {}
                Err(why) => {
                    eprintln!("Error: Failed to symlink for {}: {}", file, why);
                }
            }
        }
    }

    for dir in [
        "/data/adb/modules",
        "/data/adb/post-fs-data.d",
        "/data/adb/services.d",
    ]
    .iter()
    {
        match fs::create_dir_all(dir) {
            Ok(_) => {}
            Err(why) => {
                eprintln!("Error: Failed to create {} dir: {}", dir, why);
            }
        }
    }

    // Install magiskMan into system if missing
    if !Path::new(magisk_apk_dir).exists() {
        match fs::create_dir_all(magisk_apk_dir) {
            Ok(_) => extract_file(
                &format!("{}{}", magisk_apk_dir, "/MagiskSu.apk"),
                include_bytes!("asset/magisk.apk"),
                0o644,
            ),
            Err(why) => {
                eprintln!("Error: Failed to create MagiskApkDir dir: {}", why);
            }
        }
    }

    for su_bin in ["/system/bin/su", "/system/xbin/su"].iter() {
        if Path::new(su_bin).exists() {
            match fs::remove_file(su_bin) {
                Ok(_) => {}

                Err(why) => {
                    eprintln!(
                        "Error: Failed to remove existing {} binary: {}",
                        su_bin, why
                    );
                }
            }
        }

        /*
        match symlink("/sbin/su", su_bin) {
            Ok(_) => {}
            Err(why) => {
                eprintln!("Error: Failed to symlink for {}: {}", su_bin, why);
            }
        }
        */
    }

    //// Swtitch process to OS init.
    if Path::new(init_real).exists() {
        executev(&[init_real]);
    }
}

fn main() {}
