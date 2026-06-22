#[cfg(test)]
mod tests {
    use crate::core::text_slicer::SliceType;
    use crate::plugins::shell_session_plugin::methods::ShellSessionPlugin;
    use crate::plugins::test_utils::{compress_to_string, read_sample_file};
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    pub const SHOWCASE_CASES: &[(&str, &str)] = &[
        ("case_001_bash_success", "Bash success with command and empty prompt"),
        ("case_002_bash_command_not_found", "Bash command not found error"),
        ("case_003_bash_pipefail", "Bash pipefail error"),
        ("case_004_bash_set_x_fail", "Bash set -x trace failing"),
        ("case_005_zsh_glob_no_match", "Zsh glob no match error"),
        ("case_006_fish_syntax", "Fish syntax error"),
        ("case_007_powershell_success_table", "PowerShell successful output with table format"),
        ("case_008_powershell_parser_error", "PowerShell ParserError exception"),
        ("case_009_powershell_parameter_binding_exception", "PowerShell ParameterBindingException"),
        ("case_010_cmd_errorlevel", "CMD errorlevel output"),
        ("case_011_cmd_path_not_found", "CMD path not found error"),
        ("case_012_windows_dir_listing", "Windows CMD dir listing"),
        ("case_013_bash_ls_posix", "Bash POSIX ls -la listing"),
        ("case_014_bash_tree", "Bash tree command output"),
        ("case_015_bash_env_secrets", "Bash env with sensitive secrets"),
        ("case_016_bash_find_grep", "Bash find and grep pipeline"),
        ("case_017_bash_curl_tar", "Bash curl download and tar extraction"),
        ("case_018_powershell_get_childitem", "PowerShell Get-ChildItem listing"),
        ("case_019_powershell_invoke_webrequest", "PowerShell Invoke-WebRequest output"),
        ("case_020_cmd_xcopy", "CMD xcopy command output"),
        ("case_021_cmd_robocopy", "CMD robocopy sync trace"),
        ("case_022_ci_set_x_trace", "CI set -x docker build trace"),
        ("case_023_bash_cp_mv", "Bash cp and mv file operations"),
        ("case_024_bash_rm_rf", "Bash rm -rf recursive removal"),
        ("case_025_bash_chmod_chown", "Bash chmod and chown permission changes"),
        ("case_026_bash_find", "Bash find file search"),
        ("case_027_bash_grep_awk", "Bash grep and awk pipeline"),
        ("case_028_bash_ps", "Bash ps with grep filter"),
        ("case_029_bash_top", "Bash top process monitor with head"),
        ("case_030_bash_systemctl", "Bash systemctl systemd service control"),
        ("case_031_bash_df", "Bash df disk usage"),
        ("case_032_bash_du", "Bash du directory size"),
        ("case_033_bash_netstat", "Bash netstat network connections"),
        ("case_034_bash_ping", "Bash ping network probe"),
        ("case_035_bash_tar", "Bash tar archive operation"),
        ("case_036_bash_rsync", "Bash rsync file sync"),
        ("case_037_bash_redirection", "Bash shell redirection operators"),
        ("case_038_bash_env_prefix", "Bash env-prefix routing to dedicated tool"),
        ("case_039_ps_get_process", "PowerShell Get-Process with Select-Object"),
        ("case_040_ps_copy_item", "PowerShell Copy-Item file copy"),
        ("case_041_ps_where_object", "PowerShell Where-Object filter pipeline"),
        ("case_042_ps_convertfrom_json", "PowerShell ConvertFrom-Json parse"),
        ("case_043_cmd_copy", "CMD copy file copy"),
        ("case_044_cmd_where", "CMD where command lookup"),
        ("case_045_cmd_type", "CMD type file content display"),
        ("case_046_cmd_findstr", "CMD findstr text search"),
        ("case_047_cmd_dir_s", "CMD dir /s recursive listing"),
        ("case_048_bash_permission_denied", "Bash permission denied error"),
        ("case_049_bash_syntax_error", "Bash syntax error"),
        ("case_050_ps_command_not_found", "PowerShell command not found"),
        ("case_051_cmd_not_recognized", "CMD not recognized error"),
        ("case_052_bash_git", "Bash git routing boundary - handed off to git plugin"),
        ("case_053_ps_cargo", "PowerShell cargo routing boundary - handed off to cargo plugin"),
        ("case_054_bash_kubectl", "Bash kubectl routing boundary - handed off to kubectl plugin"),
        ("case_055_cmd_mvn", "CMD mvn routing boundary - handed off to mvn plugin"),
        ("case_056_bash_docker", "Bash docker routing boundary - handed off to docker plugin"),
        ("case_057_bash_npm", "Bash npm routing boundary - handed off to npm plugin"),
        ("case_058_bash_pytest", "Bash pytest routing boundary - handed off to pytest plugin"),
        ("case_059_bash_terraform", "Bash terraform routing boundary - handed off to terraform plugin"),
        ("case_060_ps_helm", "PowerShell helm routing boundary - handed off to helm plugin"),
        ("case_061_bash_sed_sort", "Bash sed sort uniq pipeline"),
        ("case_062_bash_jobs_kill", "Bash jobs and kill job control"),
        ("case_063_bash_ll_alias", "Bash ll alias listing"),
        ("case_064_bash_mkdir", "Bash mkdir -p directory creation"),
        ("case_065_bash_rmdir", "Bash rmdir empty directory removal"),
        ("case_066_bash_touch", "Bash touch creating empty file"),
        ("case_067_bash_ripgrep", "Bash ripgrep content search"),
        ("case_068_bash_tail", "Bash tail -n log tailing"),
        ("case_069_bash_wc", "Bash wc line/byte counting"),
        ("case_070_bash_zip", "Bash zip -r archive creation"),
        ("case_071_bash_unzip", "Bash unzip archive extraction"),
        ("case_072_bash_service", "Bash service systemd status listing"),
        ("case_073_bash_mount", "Bash mount filesystem table"),
        ("case_074_bash_scp", "Bash scp remote file copy"),
        ("case_075_cmd_ipconfig", "CMD ipconfig /all network configuration"),
        ("case_076_bash_ifconfig", "Bash ifconfig interface details"),
        ("case_077_bash_ip", "Bash ip addr show interface details"),
        ("case_078_bash_ss", "Bash ss -tulpn listening sockets"),
        ("case_079_ps_remove_item", "PowerShell Remove-Item recursive deletion"),
        ("case_080_bash_wget", "Bash wget file download"),
    ];

    #[test]
    fn test_shell_session_plugin_showcase() {
        let plugin = ShellSessionPlugin::default();
        let mut all_output = String::new();
        all_output.push_str("\n  Shell Session Plugin Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (case_id, title) in SHOWCASE_CASES {
            let file_name = format!("{}.log", case_id);
            let raw = read_sample_file("shell_session_plugin", &file_name);
            let compressed = compress_to_string(&plugin, &raw, SliceType::LogBlock);

            all_output.push_str(&format!("\nCase {} - {} ({})\n", case_id, title, file_name));
            all_output.push_str(&"-".repeat(80));
            let original_lines = raw.lines().count();
            let original_bytes = raw.len();
            let compact_lines = compressed.lines().count();
            let compact_bytes = compressed.len();
            let compression_ratio = if original_bytes > 0 {
                (1.0 - compact_bytes as f64 / original_bytes as f64) * 100.0
            } else {
                0.0
            };

            all_output.push_str(&format!(
                "\nOriginal: {} lines, {} bytes | Compact: {} lines, {} bytes | Compression: {:.1}%\n",
                original_lines, original_bytes, compact_lines, compact_bytes, compression_ratio
            ));
            
            all_output.push_str("-- Case text --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&raw);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }

            all_output.push_str("-- Compact Output (full) --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&compressed);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }
            all_output.push_str("\n");
        }

        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let report_path = PathBuf::from(manifest_dir)
            .join("target")
            .join("shell_session_plugin_compact_showcase_report.txt");
            
        // ensure target dir exists
        let _ = std::fs::create_dir_all(report_path.parent().unwrap());
        
        let mut file = File::create(&report_path).expect("Failed to create showcase report");
        file.write_all(all_output.as_bytes()).unwrap();
    }
}
