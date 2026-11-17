//! The assistant's outside tools (see `mcp.rs`): the servers the person
//! added, the tools they offer, and the page that manages them
//! (`sanaga://tools`).
//!
//! Every use of an outside tool is shown to the person and waits for them,
//! unless they let that very tool run without asking, here.

use super::*;
use crate::chat;
use crate::mcp::{self, RemoteTool, Server};
use eframe::egui::{Color32, Margin, RichText, TextEdit};

#[derive(Default)]
pub(super) struct McpForm {
    open: bool,
    name: String,
    /// A program on this computer (true), or an address.
    program: bool,
    target: String,
    token: String,
    status: String,
    failed: bool,
}

/// What is known of the servers: the list kept, and each one's tools as
/// last asked (or why they could not be).
#[derive(Default)]
pub(super) struct McpState {
    pub servers: Vec<Server>,
    pub tools: HashMap<String, Result<Vec<RemoteTool>, String>>,
    /// The servers being asked for their tools.
    asking: HashSet<String>,
    pub hub: mcp::Hub,
    /// The tools as the model names them: its name → (server, tool).
    pub names: HashMap<String, (String, String)>,
}

impl Sanaga {
    fn mcp_file(&self) -> Option<std::path::PathBuf> {
        self.store.data_dir().filter(|_| !self.guest).map(|d| d.join("mcp.tsv"))
    }

    /// Read the servers the person added, once, and ask the ones switched
    /// on for their tools (on threads).
    pub(super) fn ensure_mcp(&mut self) {
        if !self.first_time("mcp") {
            return;
        }
        let Some(file) = self.mcp_file() else { return };
        self.off_thread(
            move || std::fs::read_to_string(file).map(|t| mcp::parse_list(&t)).unwrap_or_default(),
            |app, list| {
                // (a server added while the file was being read stays)
                for s in list {
                    if !app.mcp.servers.iter().any(|had| had.name == s.name) {
                        app.mcp.servers.push(s);
                    }
                }
                for name in app.mcp.servers.iter().filter(|s| s.on).map(|s| s.name.clone()).collect::<Vec<_>>() {
                    app.ask_mcp_tools(&name);
                }
            },
        );
    }

    fn save_mcp(&mut self) {
        let Some(file) = self.mcp_file() else { return };
        let text = mcp::format_list(&self.mcp.servers);
        self.off_thread(move || crate::store::write_atomic(&file, text), |_, _| {});
        self.name_mcp_tools();
    }

    /// Ask a server which tools it offers (on a thread; its token is read
    /// from the keyring there).
    fn ask_mcp_tools(&mut self, name: &str) {
        let Some(server) = self.mcp.servers.iter().find(|s| s.name == name).cloned() else { return };
        if !self.mcp.asking.insert(server.name.clone()) {
            return;
        }
        let (hub, profile) = (self.mcp.hub.clone(), self.store.profile.clone());
        self.off_thread(
            move || {
                let token = (!server.program).then(|| crate::keyring::lookup(&profile, &format!("{}{}", mcp::KEY_PREFIX, server.name), "token")).flatten();
                // (asked afresh: the server may have changed since)
                hub.forget(&server.name);
                (hub.with(&server, token, |s| s.tools()), server.name)
            },
            |app, (tools, name)| {
                app.mcp.asking.remove(&name);
                app.mcp.tools.insert(name, tools);
                app.name_mcp_tools();
            },
        );
    }

    /// Work out the names the model knows the outside tools by.
    fn name_mcp_tools(&mut self) {
        let mut names = HashMap::new();
        for s in self.mcp.servers.iter().filter(|s| s.on) {
            if let Some(Ok(tools)) = self.mcp.tools.get(&s.name) {
                for t in tools {
                    names.entry(mcp::tool_name(&s.name, &t.name)).or_insert_with(|| (s.name.clone(), t.name.clone()));
                }
            }
        }
        self.mcp.names = names;
    }

    /// The outside tools given to a conversation at its start.
    pub(super) fn mcp_tools(&self) -> Vec<chat::Tool> {
        let mut out = Vec::new();
        for s in self.mcp.servers.iter().filter(|s| s.on) {
            let Some(Ok(tools)) = self.mcp.tools.get(&s.name) else { continue };
            for t in tools {
                let name = mcp::tool_name(&s.name, &t.name);
                if out.iter().any(|x: &chat::Tool| x.name == name) {
                    continue;
                }
                // (said to be the person's outside tool: what it returns is material to read, like a page)
                let description = format!("Outside tool \"{}\" of the person's own server \"{}\". {}", t.name, s.name, t.description);
                out.push(chat::Tool { name, description, schema: t.schema.clone(), acts: true });
            }
        }
        out
    }

    /// May this outside tool run without the person being asked?
    pub(super) fn mcp_tool_free(&self, model_name: &str) -> bool {
        self.mcp.names.get(model_name).is_some_and(|(server, tool)| self.mcp.servers.iter().any(|s| s.name == *server && s.on && s.free.contains(tool)))
    }

    /// Use an outside tool (the person has agreed, or let it free): on a
    /// thread, the answer goes back to the model's.
    pub(super) fn call_mcp_tool(&mut self, ask: super::assistant::ToolAsk) {
        let reply = |ask: super::assistant::ToolAsk, text: String, error: bool| {
            let _ = ask.reply.send(chat::ToolResult { id: ask.call.id.clone(), name: ask.call.name.clone(), text, error });
        };
        let Some((server_name, tool)) = self.mcp.names.get(&ask.call.name).cloned() else { return reply(ask, "This outside tool is no longer there.".into(), true) };
        let Some(server) = self.mcp.servers.iter().find(|s| s.name == server_name && s.on).cloned() else { return reply(ask, "This outside tool's server is switched off.".into(), true) };
        let (hub, profile) = (self.mcp.hub.clone(), self.store.profile.clone());
        self.off_thread(
            move || {
                let token = (!server.program).then(|| crate::keyring::lookup(&profile, &format!("{}{}", mcp::KEY_PREFIX, server.name), "token")).flatten();
                let input = ask.call.input.clone();
                (hub.with(&server, token, |s| s.call(&tool, &input)), ask)
            },
            move |_, (done, ask)| match done {
                Ok((text, failed)) => reply(ask, text, failed),
                Err(e) => reply(ask, format!("The tool could not be reached: {e}"), true),
            },
        );
    }

    /// sanaga://tools: the outside tools of the assistant.
    pub(super) fn page_mcp(&mut self, ui: &mut Ui, th: &Theme) {
        self.ensure_mcp();
        let can_keep = self.mcp_file().is_some();
        let (mut add, mut remove, mut toggle, mut again, mut free) = (false, None::<String>, None::<String>, None::<String>, None::<(String, String)>);
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Outils extérieurs de l'assistant"));
            ui.add(
                egui::Label::new(
                    RichText::new(tr("Ajoutez vos propres serveurs d'outils (protocole MCP) : un programme sur cet ordinateur, ou une adresse. L'assistant peut alors utiliser leurs outils dans une conversation ; chaque utilisation vous est montrée avant d'être faite, sauf pour les outils que vous laissez libres.")).size(14.0).color(th.sub),
                )
                .wrap(),
            );
            ui.add_space(14.0);
            let m = &self.mcp;
            for s in &m.servers {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width() - 250.0);
                            ui.label(RichText::new(&s.name).size(14.5).family(bold()).color(th.text));
                            ui.add(egui::Label::new(RichText::new(format!("{} : {}", if s.program { tr("Programme") } else { tr("Adresse") }, s.target)).size(12.0).color(th.sub)).truncate());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if super::chrome::confirm_button(ui, ("mcp", &s.name), tr("Supprimer")) {
                                remove = Some(s.name.clone());
                            }
                            if s.on && ui.button(tr("Relire ses outils")).clicked() {
                                again = Some(s.name.clone());
                            }
                            let mut on = s.on;
                            if super::settings::switch(ui, th, &mut on) {
                                toggle = Some(s.name.clone());
                            }
                        });
                    });
                    if !s.on {
                        return;
                    }
                    ui.add_space(4.0);
                    match m.tools.get(&s.name) {
                        _ if m.asking.contains(&s.name) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(tr("Lecture de ses outils…")).size(12.5).color(th.sub));
                            });
                        }
                        None => {}
                        Some(Err(why)) => {
                            ui.add(egui::Label::new(RichText::new(tf("Injoignable : {}", &[why])).size(12.5).color(Color32::from_rgb(0xC0, 0x38, 0x2B))).wrap());
                        }
                        Some(Ok(tools)) if tools.is_empty() => {
                            ui.label(RichText::new(tr("Ce serveur n'offre aucun outil.")).size(12.5).color(th.sub));
                        }
                        Some(Ok(tools)) => {
                            for t in tools {
                                ui.horizontal(|ui| {
                                    let mut is_free = s.free.contains(&t.name);
                                    if super::settings::switch(ui, th, &mut is_free) {
                                        free = Some((s.name.clone(), t.name.clone()));
                                    }
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&t.name).size(13.0).color(th.text));
                                        if !t.description.is_empty() {
                                            ui.add(egui::Label::new(RichText::new(&t.description).size(11.5).color(th.sub)).truncate());
                                        }
                                    });
                                });
                            }
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(RichText::new(tr("Interrupteur allumé : l'outil s'utilise sans vous demander. À réserver aux outils qui ne font que lire.")).size(11.5).color(th.sub))
                                    .wrap(),
                            );
                        }
                    }
                });
                ui.add_space(6.0);
            }
            if m.servers.is_empty() {
                ui.label(RichText::new(tr("Aucun serveur ajouté.")).size(13.0).color(th.sub));
                ui.add_space(6.0);
            }
            ui.add_space(8.0);
            let f = &mut self.mcp_form;
            if !can_keep {
                ui.label(RichText::new(tr("Une fenêtre d'invité ne garde pas d'outils.")).size(12.5).color(th.sub));
            } else if !f.open {
                if ui.add(egui::Button::new(RichText::new(tr("Ajouter un serveur")).color(Color32::WHITE)).fill(th.accent).min_size(vec2(160.0, 32.0)).corner_radius(CornerRadius::same(10))).clicked()
                {
                    *f = McpForm { open: true, program: true, ..Default::default() };
                }
            } else {
                ui.label(RichText::new(tr("Ajouter un serveur")).size(17.0).family(bold()).color(th.text));
                ui.add_space(8.0);
                ui.label(RichText::new(tr("Nom")).family(bold()).color(th.text));
                ui.add(TextEdit::singleline(&mut f.name).hint_text(tr("Ex. : Notes")).desired_width(260.0));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.program, true, tr("Un programme sur cet ordinateur"));
                    ui.radio_value(&mut f.program, false, tr("Une adresse"));
                });
                ui.add_space(4.0);
                if f.program {
                    ui.add(TextEdit::singleline(&mut f.target).hint_text(tr("Sa ligne de commande, ex. : npx -y @exemple/serveur-notes")).desired_width(f32::INFINITY));
                    ui.add(
                        egui::Label::new(
                            RichText::new(tr("Ce programme est lancé avec vos droits, comme si vous le lanciez vous-même : n'ajoutez que ceux en qui vous avez confiance.")).size(11.5).color(th.sub),
                        )
                        .wrap(),
                    );
                } else {
                    ui.add(TextEdit::singleline(&mut f.target).hint_text("https://…/mcp").desired_width(f32::INFINITY));
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr("Jeton d'accès (facultatif)")).family(bold()).color(th.text));
                    ui.add(TextEdit::singleline(&mut f.token).password(true).desired_width(f32::INFINITY));
                    ui.add(
                        egui::Label::new(
                            RichText::new(tr("Gardé dans le trousseau du système. Les services qui demandent une fenêtre de connexion (OAuth) ne sont pas encore pris en charge."))
                                .size(11.5)
                                .color(th.sub),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let name = mcp::server_name(&f.name);
                    let ready = !name.is_empty() && !f.target.trim().is_empty();
                    let b = egui::Button::new(RichText::new(tr("Ajouter")).color(Color32::WHITE))
                        .fill(if ready { th.accent } else { th.sub })
                        .min_size(vec2(120.0, 32.0))
                        .corner_radius(CornerRadius::same(10));
                    if ui.add_enabled(ready, b).clicked() {
                        add = true;
                    }
                    if ui.add(egui::Button::new(tr("Annuler")).min_size(vec2(90.0, 32.0)).corner_radius(CornerRadius::same(10))).clicked() {
                        *f = McpForm::default();
                    }
                });
            }
            let f = &self.mcp_form;
            if !f.status.is_empty() {
                ui.add_space(10.0);
                let (fill, text) = if f.failed { (Color32::from_rgb(0xD0, 0x40, 0x30).gamma_multiply(0.12), Color32::from_rgb(0xC0, 0x38, 0x2B)) } else { (th.accent.gamma_multiply(0.12), th.text) };
                Frame::new().fill(fill).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&f.status).color(text)).wrap());
                });
            }
            ui.add_space(24.0);
        });
        if add {
            let f = &self.mcp_form;
            let (name, program, target, token) = (mcp::server_name(&f.name), f.program, f.target.trim().to_string(), f.token.clone());
            if self.mcp.servers.iter().any(|s| s.name == name) {
                (self.mcp_form.status, self.mcp_form.failed) = (tr("Un serveur porte déjà ce nom.").to_string(), true);
            } else if !program && !mcp::address_ok(&target) {
                (self.mcp_form.status, self.mcp_form.failed) = (tr("Adresse refusée : https, ou un serveur sur cet ordinateur.").to_string(), true);
            } else {
                self.add_mcp_server(Server { name, program, target, on: true, free: Vec::new() }, token);
            }
        }
        if let Some(name) = remove {
            self.mcp.servers.retain(|s| s.name != name);
            self.mcp.tools.remove(&name);
            self.mcp.hub.forget(&name);
            let profile = self.store.profile.clone();
            let site = format!("{}{}", mcp::KEY_PREFIX, name);
            self.off_thread(move || crate::keyring::delete(&profile, &site, "token"), |_, ()| {});
            self.save_mcp();
        }
        if let Some(name) = toggle {
            let mut now_on = false;
            if let Some(s) = self.mcp.servers.iter_mut().find(|s| s.name == name) {
                s.on = !s.on;
                now_on = s.on;
            }
            if now_on {
                self.ask_mcp_tools(&name);
            } else {
                self.mcp.hub.forget(&name);
            }
            self.save_mcp();
        }
        if let Some(name) = again {
            self.ask_mcp_tools(&name);
        }
        if let Some((server, tool)) = free {
            if let Some(s) = self.mcp.servers.iter_mut().find(|s| s.name == server) {
                match s.free.iter().position(|t| *t == tool) {
                    Some(k) => {
                        s.free.remove(k);
                    }
                    None => s.free.push(tool),
                }
            }
            self.save_mcp();
        }
    }

    /// Add a server to the list (its token, if any, goes to the keyring),
    /// and ask it for its tools.
    pub(super) fn add_mcp_server(&mut self, server: Server, token: String) {
        let name = server.name.clone();
        self.mcp.servers.push(server);
        self.mcp_form = McpForm { status: tf("« {} » est ajouté.", &[&name]), ..Default::default() };
        self.save_mcp();
        if token.trim().is_empty() {
            self.ask_mcp_tools(&name);
        } else {
            let profile = self.store.profile.clone();
            let site = format!("{}{}", mcp::KEY_PREFIX, name);
            // (the token is kept first: the tools are asked for with it)
            self.off_thread(
                move || (crate::keyring::store(&profile, &site, "token", token.trim()), name),
                |app, (kept, name)| {
                    if !kept {
                        (app.mcp_form.status, app.mcp_form.failed) = (tr("Le jeton n'a pas pu être gardé dans le trousseau du système.").to_string(), true);
                    }
                    app.ask_mcp_tools(&name);
                },
            );
        }
    }
}
