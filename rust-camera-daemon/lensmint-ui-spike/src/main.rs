use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    // Slightly enlarge viewport to fit the new split layout, keeping embedded aspect ratio.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "LensMint Static UI",
        options,
        Box::new(|_cc| Box::new(SpikeApp::default())),
    )
}

#[derive(Default)]
struct SpikeApp {}

impl eframe::App for SpikeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Bottom Status Bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Device Identity: [Pending Phase 1]");
                // Right-aligned status indicator
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Status: Ready");
                });
            });
        });

        // 2. Right Control Panel
        egui::SidePanel::right("control_panel").min_width(120.0).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                // Prominent capture button
                let capture_btn = egui::Button::new("Capture").min_size(egui::vec2(100.0, 50.0));
                if ui.add(capture_btn).clicked() {
                    println!("Capture button clicked. (Logic pending Week 1)");
                }
            });
        });

        // 3. Central Preview Area
        egui::CentralPanel::default().show(ctx, |ui| {
            // Render a dark gray background to simulate camera feed placeholder
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(50, 50, 50));
            
            ui.centered_and_justified(|ui| {
                ui.heading("Camera Preview Placeholder");
            });
        });
        
        // Note: Event-driven only. No ctx.request_repaint() prevents CPU lockup on embedded aarch64.
    }
}