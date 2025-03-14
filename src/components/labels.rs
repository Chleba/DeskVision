use crate::{config::IMG_LABEL_PROMPT, enums::ImageBase64Search, utils::img_path_to_base64};
use ollama_rs::{
    generation::{completion::request::GenerationRequest, options::GenerationOptions},
    Ollama,
};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::{app_state::AppState, enums::BroadcastMsg};
use std::sync::{Arc, Mutex};

pub struct Labeler {
    action_tx: Option<UnboundedSender<BroadcastMsg>>,
    app_state: Option<Arc<Mutex<AppState>>>,
    files_to_label: Vec<String>,
    is_labeling: bool,
}

impl Labeler {
    pub fn new() -> Self {
        Self {
            action_tx: None,
            app_state: None,
            files_to_label: vec![],
            is_labeling: false,
        }
    }

    fn start_labeling(&mut self) {
        println!("START LABELING ---- ");
        let mut dir_files = vec![];
        {
            if let Some(ref app_state) = self.app_state {
                dir_files = app_state.lock().unwrap().dir_files.clone();
            }
        }

        // -- get all files that is not having labels
        let mut all_files = vec![];
        for dir in dir_files.iter() {
            for file in dir.files_with_labels.iter() {
                if file.labels.is_empty() {
                    all_files.push(file.file.to_string());
                }
            }
        }

        self.files_to_label = all_files;

        self.next_vision_search();
    }

    fn next_vision_search(&mut self) {
        if !self.is_labeling {
            return;
        }
        if let Some(img) = self.files_to_label.pop() {
            self.label_image(img);
        } else {
            self.finished_image_search();
        }
    }

    fn finished_image_search(&mut self) {
        println!("FINISHED LABELING ---");
        self.is_labeling = false;
        if let Some(action_tx) = self.action_tx.clone() {
            let _ = action_tx.send(BroadcastMsg::FinishLabeling);
        }
    }

    fn get_vision_model(&mut self) -> Option<String> {
        if let Some(app_state) = self.app_state.clone() {
            let a_state = app_state.lock().unwrap();
            let v_models = a_state.ollama_state.get_vision_models();
            if !v_models.is_empty() {
                let model_name = v_models[0].name.clone();
                println!("SELECTED FIRST VISION MODEL: {}", model_name.clone());
                return Some(model_name);
            }
        }
        None
    }

    fn label_image(&mut self, file: String) {
        println!("> start labeling img: {}", file);
        if let Some(img) = img_path_to_base64(file.clone()) {
            if let Some(vision_model) = self.get_vision_model() {
                self.msg_to_vision(file, vision_model, IMG_LABEL_PROMPT.to_string(), img);
            } else {
                println!("NO VISION MODEL FOUND");
            }
        }
    }

    fn msg_to_vision(
        &mut self,
        file: String,
        model_name: String,
        prompt: String,
        img: ImageBase64Search,
    ) {
        println!("> send img to vision: {}", file);
        let (url, port) = self.get_ollama_url(self.app_state.clone());
        let ollama = Ollama::new(url, port);
        if let Some(action_tx) = self.action_tx.clone() {
            tokio::spawn(async move {
                let res = ollama
                    .generate(
                        GenerationRequest::new(model_name, prompt)
                            .add_image(img.clone().base64)
                            .options(GenerationOptions::default().temperature(0.0)),
                    )
                    .await;
                if let Ok(resp) = res {
                    println!("{:?} desc vision search", &resp.response);
                    let _ = action_tx
                        .send(BroadcastMsg::GetLabelsForImage(file, resp.response.clone()));
                }
            });
        }
    }
}

impl Component for Labeler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update(&mut self, msg: BroadcastMsg) {
        match msg {
            BroadcastMsg::StartLabeling => {
                self.is_labeling = true;
                self.start_labeling();
            }

            BroadcastMsg::StopLabeling => {
                self.is_labeling = false;
            }

            BroadcastMsg::GetLabelsForImage(_file, _labels) => {
                self.next_vision_search();
            }
            _ => {}
        }
    }

    fn render(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after_secs(1.0);
    }

    fn register_app_state(&mut self, app_state: Arc<Mutex<AppState>>) {
        self.app_state = Some(app_state);
    }

    fn register_tx(&mut self, action_tx: UnboundedSender<BroadcastMsg>) {
        self.action_tx = Some(action_tx);
    }
}
