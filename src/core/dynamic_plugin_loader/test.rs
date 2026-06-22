//! Tests for dynamic plugin loading

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::path::PathBuf;

    #[test]
   fn test_plugin_interface_version() {
        // Verify the plugin interface version is defined
        assert_eq!(plugin_interface::PLUGIN_API_VERSION, 1);
    }

    #[test]
   fn test_dynamic_plugin_loader_creation() {
      let loader = DynamicPluginLoader::new(PathBuf::from("plugins"));
        assert_eq!(loader.loaded_plugin_names().len(), 0);
    }

    #[test]
   fn test_dynamic_plugin_config_deserialization() {
      let toml_str = r#"
            [[dynamic_plugins]]
           name = "gcc_log"
            file = "gcc_log_plugin.dll"
            enabled = true
            
            [[dynamic_plugins]]
           name = "test_plugin"
            file = "test.dll"
            enabled = false
        "#;
        
      let config: toml::Value = toml::from_str(toml_str).unwrap();
      let plugins = config.get("dynamic_plugins").unwrap().as_array().unwrap();
        
        assert_eq!(plugins.len(), 2);
        
      let first = &plugins[0].as_table().unwrap();
        assert_eq!(first.get("name").unwrap().as_str().unwrap(), "gcc_log");
        assert_eq!(first.get("enabled").unwrap().as_bool().unwrap(), true);
        
      let second = &plugins[1].as_table().unwrap();
        assert_eq!(second.get("enabled").unwrap().as_bool().unwrap(), false);
    }
}
