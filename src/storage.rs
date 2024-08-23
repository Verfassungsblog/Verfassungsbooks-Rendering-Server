use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::Path;
use std::sync::{Arc, RwLock};
use vb_exchange::{RenderingRequest, RenderingStatus};
use vb_exchange::export_formats::ExportFormat;
use crate::settings::Settings;

pub struct Storage{
    pub request_queue: Arc<RwLock<VecDeque<RenderingRequest>>>,
    pub request_status: Arc<RwLock<HashMap<uuid::Uuid, RenderingStatus>>>,
    /// Contains a HashMap with template_id as key, template_version_id as value.
    pub template_storage: Arc<RwLock<HashMap<uuid::Uuid, TemplateStorageEntry>>>
}

pub struct TemplateStorageEntry{
    pub version_id: uuid::Uuid,
    pub export_formats: HashMap<String, ExportFormat>
}

impl Storage{
    pub fn new() -> Storage{
        Storage{
            request_queue: Arc::new(Default::default()),
            request_status: Arc::new(Default::default()),
            template_storage: Arc::new(Default::default()),
        }
    }
}

/// Removes all files from temp template dir
pub fn clear_template_dir(settings: &Settings) -> io::Result<()>{
    let entries = std::fs::read_dir(Path::new(&settings.temp_template_path.clone()))?;

    for entry in entries{
        let entry = entry?.path();
        if !entry.is_dir(){
            std::fs::remove_file(entry)?;
        }else{
            std::fs::remove_dir_all(entry)?;
        }
    }

    Ok(())
}