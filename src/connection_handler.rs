use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::TlsStream;
use vb_exchange::{CommunicationError, Message, RenderingError, RenderingStatus, TemplateDataRequest};
use crate::settings::Settings;
use crate::storage::{Storage, TemplateStorageEntry};

pub async fn process_connection(mut tls_stream: TlsStream<TcpStream>, storage: Arc<Storage>, settings: Arc<Settings>){
    let template_storage = storage.template_storage.clone();
    let request_storage = storage.request_queue.clone();
    let status_storage = storage.request_status.clone();

    // Get rendering request
    let rendering_request = match vb_exchange::read_message(&mut tls_stream).await{
        Ok(msg) => {
            if let Message::RenderingRequest(req) = msg{
                req
            }else{
                eprintln!("Received unexpected Message type, closing connection.");
                let _ = vb_exchange::send_message(&mut tls_stream, Message::CommunicationError(CommunicationError::UnexpectedMessageType)).await;
                return;
            }
        },
        Err(_) => {
            eprintln!("Error occured, closed connection.");
            return;
        }
    };

    let request_id = rendering_request.request_id.clone();

    status_storage.write().unwrap().insert(rendering_request.request_id.clone(), RenderingStatus::SendToRenderingServer);

    // Check if we have the template already stored (in the right version)
    let template_stored = match template_storage.read().unwrap().get(&rendering_request.template_id){
        Some(res) => {
            res.version_id == rendering_request.template_version_id
        },
        None => false
    };

    if !template_stored{
        // Update status
        if let Some(status) = status_storage.write().unwrap().get_mut(&rendering_request.request_id){
            *status = RenderingStatus::RequestingTemplate
        }

        // Request template from main server
        if let Err(_) = vb_exchange::send_message(&mut tls_stream, Message::TemplateDataRequest(TemplateDataRequest{ template_id: rendering_request.template_id, template_version_id: rendering_request.template_version_id })).await{
            eprintln!("Error occured requesting template data. Closing connection");
            return;
        }
        let template_data = match vb_exchange::read_message(&mut tls_stream).await {
            Ok(msg) => {
                if let Message::TemplateDataResult(msg) = msg {
                    msg
                } else {
                    eprintln!("Received unexpected Message type, closing connection.");
                    let _ = vb_exchange::send_message(&mut tls_stream, Message::CommunicationError(CommunicationError::UnexpectedMessageType)).await;
                    return;
                }
            }
            Err(_) => {
                eprintln!("Error occured, closed connection.");
                return;
            }
        };
        if template_data.template_id != rendering_request.template_id || template_data.template_version_id != rendering_request.template_version_id{
            eprintln!("Received unexpected template data, closing connection.");
            let _ = vb_exchange::send_message(&mut tls_stream, Message::CommunicationError(CommunicationError::WrongTemplateDataSend)).await;
            return;
        }

        let res = tokio::task::spawn_blocking(move || {
            match template_data.contents.to_file(PathBuf::from(&settings.temp_template_path).join(template_data.template_version_id.to_string())){
                Ok(_) => {
                    Ok(())
                },
                Err(e) => {
                    eprintln!("Couldn't save template data to file: {}", e);
                    Err(())
                }
            }
        }).await;

        match res{
            Ok(res2) => match res2{
                Ok(_) => {}
                Err(_) => {
                    return
                }
            },
            Err(e) => {
                eprintln!("Couldn't join: {}", e);
                return
            }
        }

        let entry = TemplateStorageEntry{
            version_id: rendering_request.template_version_id,
            export_formats: template_data.export_formats,
        };
        template_storage.write().unwrap().insert(rendering_request.template_id, entry);
    }

    request_storage.write().unwrap().push_front(rendering_request);

    // Fetch status of our rendering_request and send status updates
    loop{
        let status = match status_storage.read().unwrap().get(&request_id){
            Some(res) => res.clone(),
            None => {
                RenderingStatus::Failed(RenderingError::Other("Not Found".to_string()))
            }
        };
        //println!("Debug: Status {:?}", status);
        match status{
            // break if finished or failed
            RenderingStatus::Finished(_) => {
                if let Err(_) = vb_exchange::send_message(&mut tls_stream, Message::RenderingRequestStatus(status)).await{
                    eprintln!("Couldn't send result to server. Closing connection");
                }
                break;
            }
            RenderingStatus::Failed(_) => {
                if let Err(_) = vb_exchange::send_message(&mut tls_stream, Message::RenderingRequestStatus(status)).await{
                    eprintln!("Couldn't send result to server. Closing connection");
                }
                break;
            },
            _ => {
                if let Err(_) = vb_exchange::send_message(&mut tls_stream, Message::RenderingRequestStatus(status)).await{
                    eprintln!("Couldn't send status update to server. Closing connection");
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Remove status
    let _ = status_storage.write().unwrap().remove(&request_id);
}