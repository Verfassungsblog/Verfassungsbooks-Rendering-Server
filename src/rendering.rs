use std::{fs, io};
use std::hash::Hasher;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use handlebars::{Context, DirectorySourceOptions, Handlebars, Helper, HelperResult, JsonRender, Output, RenderContext, RenderError, RenderErrorReason};
use image::Luma;
use qrcode::QrCode;
use tokio::task::JoinSet;
use vb_exchange::{FilesOnMemoryOrHarddrive, NamedFile, RenderingError, RenderingRequest, RenderingResult, RenderingStatus};
use vb_exchange::export_formats::{ExportStepData, PandocExportStep, RawExportStep, VivliostyleExportStep};
use vb_exchange::projects::PreparedProject;
use crate::settings::Settings;
use crate::storage::Storage;

pub async fn rendering_worker(storage: Arc<Storage>, settings: Arc<Settings>) {
    let subthreads_num = Arc::new(AtomicU64::new(0));

    loop{
        if subthreads_num.load(Ordering::Relaxed) >= settings.max_rendering_threads {
            println!("Too many running subthreads, waiting for one to end.");
            continue;
        }
        let next_job = storage.request_queue.write().unwrap().pop_front();

        if let Some(job) = next_job{
            println!("Found RenderingRequest.");

            let render_request = Arc::new(job);
            let storage_cpy = Arc::clone(&storage);
            let subthreads_num_cpy = Arc::clone(&subthreads_num);

            tokio::spawn(async move{
                subthreads_num_cpy.fetch_add(1, Ordering::Relaxed);

                // Get export formats to render
                let mut export_formats_queue = render_request.export_formats.clone();
                let request_status_storage = storage_cpy.request_status.clone();

                // Update status
                if let Some(status) = request_status_storage.write().unwrap().get_mut(&render_request.request_id){
                    *status = RenderingStatus::Running
                }

                let mut results: Vec<ExportFormatRenderingResult> = Vec::new();

                let mut join_set = JoinSet::new();

                while export_formats_queue.len() > 0{
                    let export_format_slug = export_formats_queue.pop().unwrap();
                    let render_request_cpy = Arc::clone(&render_request);
                    let storage_cpy2 = storage_cpy.clone();

                    println!("Debug: Started rendering export format {}.", &export_format_slug);

                    join_set.spawn(tokio::task::spawn_blocking(move || {
                        match render_export_format(export_format_slug, Arc::clone(&storage_cpy2), Arc::clone(&render_request_cpy)){
                            Ok(res) => {
                                Ok(res)
                            },
                            Err(e) => {
                                eprintln!("Couldn't render export format: {:?}", e);
                                Err(e)
                            }
                        }
                    }));
                }

                while let Some(res) = join_set.join_next().await{
                    if let Ok(res) = res{
                        if let Ok(res) = res{
                            match res{
                                Ok(res) => {
                                    results.push(res)
                                }
                                Err(e) => {
                                    eprintln!("Export Format failed rendering: {:?}", e);
                                    // Update status
                                    if let Some(status) = storage_cpy.request_status.write().unwrap().get_mut(&render_request.request_id){
                                        *status = RenderingStatus::Failed(e)
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }

                let mut res_files : Vec<NamedFile> = vec![];
                // Load result files into memory, then delete files
                for res in results{
                    for file in &res.files_to_transfer{
                        let content = match tokio::fs::read(file).await {
                            Ok(data) => data,
                            Err(e) => {
                                eprintln!("Failed to read the file: {}", e);
                                continue;
                            }
                        };
                        let filename = file.file_name().unwrap_or("invalid_filename".as_ref()).to_string_lossy().to_string();
                        res_files.push(NamedFile{ name: filename, content })
                    }
                    for dir in &res.temp_dirs{
                        if let Err(e) = tokio::fs::remove_dir_all(dir).await{
                            eprintln!("Couldn't delete temp dir: {}. Keeping it for now.", e);
                        }
                    }
                }

                // Delete project uploads
                if let FilesOnMemoryOrHarddrive::Harddrive(path) = &render_request.project_uploaded_files{
                    if let Err(e) = tokio::fs::remove_dir_all(path).await{
                        eprintln!("Couldn't delete project uploads dir: {}.", e);
                    }
                }

                // Update status
                if let Some(status) = request_status_storage.write().unwrap().get_mut(&render_request.request_id){
                    *status = RenderingStatus::Finished(RenderingResult{files: res_files})
                }

                subthreads_num_cpy.fetch_sub(1, Ordering::Relaxed);
            });

        }else{
            println!("No Rendering Job in queue");
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[derive(Clone)]
struct ExportFormatRenderingResult{
    /// Paths to all files that should be transferred to main server
    files_to_transfer: Vec<PathBuf>,
    /// Paths to all created temp directories to delete after file transfer
    temp_dirs: Vec<PathBuf>,
}

pub fn render_export_format(slug: String, storage: Arc<Storage>, request: Arc<RenderingRequest>) -> Result<ExportFormatRenderingResult, RenderingError>{
    let mut rendering_log = String::new();

    let export_format = match storage.template_storage.read().unwrap().get(&request.template_id){
        Some(template) => {
            match template.export_formats.get(&slug){
                Some(ef) => ef.clone(),
                None => {
                    eprintln!("Couldn't find export format {}.", slug);
                    return Err(RenderingError::TemplateNotFound)
                }
            }
        },
        None => {
            eprintln!("Couldn't find template {} in storage.", &request.template_id);
            return Err(RenderingError::TemplateNotFound)
        }
    };

    rendering_log.push_str(&format!("Started rendering export format {}.", export_format.slug));

    let mut temp_directories : Vec<PathBuf> = Vec::new();
    let mut files_to_copy_into_next_export_steps: Vec<PathBuf> = Vec::new();

    for export_step in export_format.export_steps{
        rendering_log.push_str(&format!("Started rendering export step {}.", export_step.name));
        let files_to_keep = export_step.files_to_keep;

        // Prepare temp directory
        let temp_directory = match prepare_temp_directory(request.clone(), &export_format.slug){
            Ok(temp_id) => temp_id,
            Err(e) => {
                eprintln!("Couldn't prepare temp directory: {}", e);
                return Err(RenderingError::Other("IO Error preparing temp directory.".to_string()));
            }
        };
        temp_directories.push(temp_directory.clone());
        rendering_log.push_str("Prepared temporary directory.");
        println!("Debug: Prepared temporary directory under {}.", &temp_directory.to_string_lossy());

        // Copy files from previous export step if any
        if files_to_copy_into_next_export_steps.len() > 0{
            for file_to_copy in &files_to_copy_into_next_export_steps {
                let filename = match file_to_copy.file_name(){
                    Some(filename) => filename.to_string_lossy().to_string(),
                    None => return Err(RenderingError::Other("Couldn't parse file name.".to_string()))
                };
                if let Err(e) = fs::copy(file_to_copy, temp_directory.join(filename.clone())){
                    rendering_log.push_str(&format!("Couldn't copy file to keep to new export step temp directory: {}", e.to_string()));
                    return Err(RenderingError::MissingExpectedFileToKeep(filename, rendering_log))
                }
            }

        }

        let res = match export_step.data{
            ExportStepData::Raw(raw) => render_raw_export_step(raw, &temp_directory, &request.prepared_project, &mut rendering_log),
            ExportStepData::Vivliostyle(vivlio) => render_vivliostyle_export_step(vivlio, &temp_directory, &mut rendering_log),
            ExportStepData::Pandoc(pan) => render_pandoc_export_step(pan, &temp_directory, &mut rendering_log)
        };

        if let Err(e) = res{
            return Err(e);
        }

        for file in files_to_keep{
            let path = temp_directory.clone().join(PathBuf::from(file.clone()));
            if !path.exists(){
                return Err(RenderingError::MissingExpectedFileToKeep(file, rendering_log))
            }else{
                files_to_copy_into_next_export_steps.push(path);
            }
        }
    }

    let res = ExportFormatRenderingResult{
        files_to_transfer: files_to_copy_into_next_export_steps,
        temp_dirs: temp_directories,
    };

    Ok(res)
}

/// Prepares a new directory inside temp, copying all global_assets and assets of the given export format to this folder
///
/// Returns a PathBuf to the temp directory
fn prepare_temp_directory(request: Arc<RenderingRequest>, export_format_slug: &str) -> io::Result<PathBuf>{
    // Prepare temp dir:
    // Create new dir in temp/
    let random_id = uuid::Uuid::new_v4();
    let temp_dir_path = format!("temp/{}", &random_id);
    let temp_dir_path = Path::new(&temp_dir_path);
    fs::create_dir(temp_dir_path)?;

    let base_dir = format!("templates/{}", &request.template_version_id);
    let base_dir = Path::new(&base_dir);

    // Copy global assets
    copy_dir_all(base_dir.join("assets"), temp_dir_path.join("global_assets"))?;

    // Copy export format specific assets
    let dir_content = fs::read_dir(base_dir.join(format!("formats/{}", export_format_slug)))?;

    // Copy project uploads
    if let vb_exchange::FilesOnMemoryOrHarddrive::Harddrive(path) = &request.project_uploaded_files{
        copy_dir_all(path, temp_dir_path.join("uploads"))?;
    }

    for entry in dir_content{
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir(){
            copy_dir_all(entry.path(), temp_dir_path)?;
        }else{
            fs::copy(entry.path(), temp_dir_path.join(entry.file_name()))?;
        }
    }

    Ok(PathBuf::from(temp_dir_path))
}

/// Copies all contents from src dir to dst dir, creating the dst dir if necessary
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub fn render_raw_export_step(step: RawExportStep, temp_dir: &PathBuf, prepared_project: &PreparedProject, rendering_log: &mut String) -> Result<(), RenderingError>{
    let mut handlebars = Handlebars::new();

    let mut dir_options = DirectorySourceOptions::default();
    dir_options.tpl_extension = String::from(".hbs.html");

    if let Err(e) = handlebars.register_templates_directory(temp_dir, dir_options){
        eprintln!("Couldn't register templates: {}", e);
        return Err(RenderingError::CouldntLoadHandlebarTemplates(e.to_string()))
    }

    // Add custom handler for qr codes
    handlebars.register_helper("qrcode", Box::new(handlebars_qrcode_helper));

    rendering_log.push_str("Starting handlebars rendering.");
    match handlebars.render(&step.entry_point.replace(".hbs.html", ""), prepared_project){
        Ok(res) => {
            if let Err(e) = fs::write(temp_dir.join(PathBuf::from(step.output_file)), res){
                eprintln!("Couldn't write rendered template: {}", e);
                rendering_log.push_str(&format!("Couldn't write rendered template: {}", e));
                return Err(RenderingError::HandlebarsRenderingFailed(rendering_log.clone()))
            }
        },
        Err(e) => {
            eprintln!("Handlebars rendering failed: {}", e);
            rendering_log.push_str(&format!("Handlebars rendering failed: {}", e));
            return Err(RenderingError::HandlebarsRenderingFailed(rendering_log.clone()));
        }
    }

    Ok(())
}

fn handlebars_qrcode_helper(h: &Helper, _: &Handlebars, _: &Context, _rc: &mut RenderContext, out: &mut dyn Output) -> HelperResult{
    let param = h.param(0).ok_or(RenderErrorReason::ParamNotFoundForIndex("qrcode", 0))?;

    let val : String = param.value().render();

    let qr_code = match QrCode::new(val.to_string()){
        Ok(qr_code) => qr_code,
        Err(e) => {
            eprintln!("Couldn't create qr code: {}", e);
            return Err(RenderError::from(RenderErrorReason::Other(format!("Couldn't create qr code: {}", e))));
        }
    };

    let image = qr_code.render::<Luma<u8>>().build();
    let image = image::DynamicImage::from(image);
    let mut buf = Cursor::new(Vec::new());
    match image.write_to(&mut buf, image::ImageFormat::Jpeg){
        Ok(_) => {}
        Err(e) => {
            eprintln!("Couldn't write qr code to buffer: {}", e);
            return Err(RenderError::from(RenderErrorReason::Other(format!("Couldn't write qr code to buffer: {}", e))));
        }
    }
    let encoded_image = BASE64_STANDARD.encode(buf.get_ref());

    out.write(&format!("<img class=\"qrcode\" src=\"data:image/jpeg;base64,{}\" alt=\"QR Code\" />", encoded_image))?;
    Ok(())
}

pub fn render_vivliostyle_export_step(step: VivliostyleExportStep, temp_dir: &PathBuf, rendering_log: &mut String) -> Result<(), RenderingError>{
    // Start bubblewrap
    let mut command = Command::new("bwrap");

    command.arg("--unshare-all").arg("--tmpfs").arg("/tmp").arg("--ro-bind").arg("/lib").arg("/lib").arg("--ro-bind").arg("/lib64").arg("/lib64").arg("--ro-bind").arg("/usr/lib").arg("/usr/lib").arg("--proc").arg("/proc").arg("--dev").arg("/dev");

    command.arg("--bind").arg(temp_dir).arg("/data").arg("--ro-bind").arg("rendering-envs/vivliostyle").arg("/env").arg("/env/node").arg("/env/node_modules/.bin/vivliostyle").arg("build").arg(format!("/data/{}", step.input_file));

    if Path::new("/usr/share/fonts").exists(){
        command.arg("--ro-bind").arg("/usr/share/fonts").arg("/usr/share/fonts");
    }
    if Path::new("/usr/local/share/fonts").exists(){
        command.arg("--ro-bind").arg("/usr/local/share/fonts").arg("/usr/share/fonts/more");
    }

    if step.press_ready{
        command.arg("-p");
    }

    command.arg("-o").arg(format!("/data/{}", step.output_file));
    command.arg("--executable-browser").arg("/env/chromium/chrome");

    match command.output() {
        Ok(res) => {
            let res = format!("Vivliostyle ran. stdout: {:?}, stderr: {:?}", String::from_utf8(res.stdout), String::from_utf8(res.stderr));
            rendering_log.push_str(&res);
            if !res.contains("Built successfully"){
                return Err(RenderingError::VivliostyleRenderingFailed(rendering_log.clone()))
            }
            Ok(())
        },
        Err(e) => {
            rendering_log.push_str(&format!("Couldn't run vivliostyle: {}", e));
            Err(RenderingError::VivliostyleRenderingFailed(rendering_log.clone()))
        }
    }
}

pub fn render_pandoc_export_step(step: PandocExportStep, temp_dir: &PathBuf, rendering_log: &mut String) -> Result<(), RenderingError>{
    println!("Started rendering pandoc export step.");
    let mut command = Command::new("bwrap");

    command.arg("--unshare-all").arg("--bind").arg(temp_dir).arg("/data").arg("--ro-bind").arg("rendering-envs/pandoc").arg("/env").arg("/env/pandoc");

    command.arg("-o").arg(format!("/data/{}", step.output_file)).arg("-t").arg(step.output_format.to_string());
    command.arg("-f").arg(step.input_format.to_string());

    if let Some(shift) = step.shift_heading_level_by{
        command.arg(format!("--shift-heading-level-by={}", shift));
    }
    if let Some(metadata_file) = step.metadata_file{
        command.arg(format!("--metadata-file={}", metadata_file));
    }
    if let Some(epub_cover_image_path) = step.epub_cover_image_path{
        command.arg(format!("--epub-cover-image={}", epub_cover_image_path));
    }
    if let Some(epub_title_page) = step.epub_title_page{
        if epub_title_page{
            command.arg("--epub-title-page=true");
        }else{
            command.arg("--epub-title-page=false");
        }
    }
    if let Some(epub_metadata_file) = step.epub_metadata_file{
        command.arg(format!("--epub-metadata={}", epub_metadata_file));
    }
    if let Some(epub_embed_fonts) = step.epub_embed_fonts{
        for font in epub_embed_fonts{
            command.arg(format!("--epub-embed-font={}", font));
        }
    }

    command.arg(format!("data/{}", step.input_file));

    match command.output() {
        Ok(res1) => {
            let stdout = String::from_utf8(res1.stdout).unwrap_or("".to_string());
            let stderr = String::from_utf8(res1.stderr).unwrap_or("".to_string());
            let res = format!("Pandoc ran. stdout: {:?}, stderr: {:?}", &stdout, &stderr);
            rendering_log.push_str(&res);
            println!("{}", res);
            Ok(())
        },
        Err(e) => {
            rendering_log.push_str(&format!("Couldn't start pandoc: {}", e));
            Err(RenderingError::PandocConversionFailed(rendering_log.clone()))
        }
    }
}