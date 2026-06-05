use eframe::egui::{
    self, Color32, Frame, Grid, Margin, RichText, ScrollArea, Sense, Stroke, TextEdit, Vec2,
};

use super::{
    app::{ApiWalletConsoleApp, LogFilter, ObserveTab},
    client::ClientStatus,
};

pub fn draw(app: &mut ApiWalletConsoleApp, ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = Color32::from_rgb(246, 247, 249);
    visuals.panel_fill = Color32::from_rgb(246, 247, 249);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(235, 238, 242);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(220, 229, 238);
    visuals.widgets.active.bg_fill = Color32::from_rgb(202, 216, 230);
    ctx.set_visuals(visuals);

    egui::TopBottomPanel::top("console_top_bar").exact_height(72.0).show(ctx, |ui| {
        ui.add_space(8.0);
        draw_top_bar(app, ui);
    });

    egui::TopBottomPanel::bottom("console_logs")
        .resizable(true)
        .default_height(240.0)
        .min_height(140.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            draw_messages_panel(app, ui);
        });

    egui::SidePanel::left("console_clients")
        .resizable(true)
        .default_width(330.0)
        .min_width(260.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            draw_clients_panel(app, ui);
        });

    egui::SidePanel::right("console_operations_v2")
        .resizable(true)
        .default_width(560.0)
        .min_width(420.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ScrollArea::vertical()
                .id_source("console_operations_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    draw_operations_panel(app, ui);
                });
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(8.0);
        ScrollArea::vertical().id_source("console_main_scroll").auto_shrink([false, false]).show(
            ui,
            |ui| {
                draw_observe_panel(app, ui);
            },
        );
    });

    draw_transfer_confirm_dialog(app, ctx);
}

fn draw_top_bar(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    Frame::none()
        .fill(Color32::from_rgb(32, 43, 54))
        .rounding(6.0)
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("API Wallet Console").size(22.0).color(Color32::WHITE).strong(),
                );
                ui.separator();
                let selected_name =
                    app.selected_client().map(|client| client.name.as_str()).unwrap_or("none");
                ui.label(
                    RichText::new(format!("Selected: {selected_name}"))
                        .color(Color32::from_rgb(210, 220, 230)),
                );
                if app.is_busy {
                    ui.label(RichText::new("Running").color(Color32::from_rgb(255, 206, 86)));
                }
                if let Some(sn) = app
                    .selected_client()
                    .and_then(|client| client.runtime_info.as_ref())
                    .map(|info| info.device_sn.as_str())
                {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("SN: {sn}")).color(Color32::from_rgb(210, 220, 230)),
                    );
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Chain: {}", app.transfer_form.chain_code))
                        .color(Color32::from_rgb(210, 220, 230)),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!("Subwallet: {}", app.transfer_form.sub_wallet_address))
                        .color(Color32::from_rgb(210, 220, 230)),
                );
                ui.separator();
                ui.label(
                    RichText::new("Mode: worker process per client")
                        .color(Color32::from_rgb(145, 220, 170)),
                );
            });
        });
}

fn draw_clients_panel(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    panel(ui, "Clients", |ui| {
        let mut selected = app.selected_client;
        let mut next_selected = None;
        let mut start_selected_requested = false;
        for client in app.clients.iter_mut() {
            let is_selected = selected == client.id;
            let fill = if is_selected {
                Color32::from_rgb(220, 235, 247)
            } else {
                Color32::from_rgb(252, 253, 255)
            };
            Frame::none()
                .fill(fill)
                .stroke(Stroke::new(1.0, Color32::from_rgb(218, 224, 232)))
                .rounding(6.0)
                .inner_margin(Margin::same(8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(is_selected, RichText::new(&client.name).strong())
                            .clicked()
                        {
                            next_selected = Some(client.id);
                            selected = client.id;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(status_badge(&client.status));
                        });
                    });
                    ui.add_space(4.0);
                    small_field(ui, "config", &mut client.config_file);
                    small_field(ui, "password", &mut client.password);
                    if let Some(error) = &client.last_error {
                        ui.colored_label(Color32::from_rgb(190, 45, 45), error);
                    }
                    if is_selected && ui.button("Start this client").clicked() {
                        start_selected_requested = true;
                    }
                });
            ui.add_space(8.0);
        }
        if let Some(next_selected) = next_selected {
            app.select_client(next_selected);
        }
        if start_selected_requested {
            app.start_selected_client();
        }

        ui.add_space(4.0);
        ui.label(
            RichText::new("Select a client to edit config/password and start it.")
                .color(Color32::from_rgb(105, 118, 132)),
        );
    });
}

fn draw_observe_panel(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        observe_tab(ui, app, ObserveTab::Runtime, "Runtime");
        observe_tab(ui, app, ObserveTab::Wallets, "Wallets");
        observe_tab(ui, app, ObserveTab::Balances, "Balances");
        observe_tab(ui, app, ObserveTab::Accounts, "Accounts");
    });
    ui.add_space(10.0);

    match app.observe_tab {
        ObserveTab::Runtime => panel(ui, "Client Runtime", |ui| {
            draw_runtime_info(app, ui);
        }),
        ObserveTab::Wallets => panel(ui, "API Wallets", |ui| {
            draw_api_wallet_table(app, ui);
        }),
        ObserveTab::Balances => panel(ui, "Balances", |ui| {
            if app.api_wallet_rows.is_empty() {
                ui.label(
                    RichText::new("Start the client and load API wallets first.")
                        .color(Color32::from_rgb(130, 140, 150)),
                );
            } else {
                let wallets = app.api_wallet_rows.clone();
                for wallet in wallets {
                    ui.add_space(8.0);
                    draw_balance_table_for_wallet(app, ui, &wallet);
                }
            }
        }),
        ObserveTab::Accounts => panel(ui, "Account Addresses", |ui| {
            if app.api_wallet_rows.is_empty() {
                ui.label(
                    RichText::new("Start the client and load API wallets first.")
                        .color(Color32::from_rgb(130, 140, 150)),
                );
            } else {
                draw_subwallet_comma_copy_controls(app, ui);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} rows", app.account_address_rows.len()))
                            .color(Color32::from_rgb(105, 118, 132)),
                    );
                    if ui.button("Copy details").clicked() {
                        ui.output_mut(|output| output.copied_text = account_details_text(app));
                    }
                    if ui.button("Copy addresses").clicked() {
                        ui.output_mut(|output| {
                            output.copied_text = app.account_addresses_only.clone()
                        });
                    }
                });
                egui::Resize::default()
                    .id_source("account_addresses_resize")
                    .default_width(ui.available_width())
                    .default_height(620.0)
                    .min_height(220.0)
                    .max_width(ui.available_width())
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_source("account_addresses_groups_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let wallets = app.api_wallet_rows.clone();
                                for wallet in wallets {
                                    ui.add_space(8.0);
                                    draw_account_address_table_for_wallet(app, ui, &wallet);
                                }
                            });
                    });
            }
        }),
    }
}

fn observe_tab(ui: &mut egui::Ui, app: &mut ApiWalletConsoleApp, tab: ObserveTab, label: &str) {
    if ui.selectable_label(app.observe_tab == tab, label).clicked() {
        app.observe_tab = tab;
    }
}

fn draw_operations_panel(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    let client_ready = app.selected_client_ready();
    panel(ui, "Actions", |ui| {
        let selected_name =
            app.selected_client().map(|client| client.name.as_str()).unwrap_or("none");
        ui.label(
            RichText::new(format!("Target client: {selected_name}"))
                .color(Color32::from_rgb(105, 118, 132)),
        );
        if app.is_busy {
            ui.label(RichText::new("Command running").color(Color32::from_rgb(166, 116, 0)));
        }
    });

    ui.add_space(10.0);
    egui::CollapsingHeader::new("Wallet").default_open(false).show(ui, |ui| {
        panel(ui, "Wallet", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        client_ready && !app.is_busy,
                        egui::Button::new("Import configured wallets"),
                    )
                    .clicked()
                {
                    app.import_wallets_for_selected_client();
                }
                if ui
                    .add_enabled(
                        client_ready && !app.is_busy,
                        egui::Button::new("Refresh wallets from DB"),
                    )
                    .clicked()
                {
                    app.refresh_wallets_for_selected_client();
                }
            });
        });
    });

    ui.add_space(10.0);
    egui::CollapsingHeader::new("Bind").default_open(false).show(ui, |ui| {
        panel(ui, "Bind", |ui| {
            two_column_fields(ui, |ui| {
                field(ui, "App ID", &mut app.bind_form.app_id);
                field(ui, "Org ID", &mut app.bind_form.org_id);
                field(ui, "Sub UID", &mut app.bind_form.subaccount_uid);
                field(ui, "Withdraw UID", &mut app.bind_form.withdrawal_uid);
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(client_ready && !app.is_busy, egui::Button::new("Import bind"))
                    .clicked()
                {
                    app.import_bind_for_selected_client();
                }
                if ui
                    .add_enabled(client_ready && !app.is_busy, egui::Button::new("Scan bind"))
                    .clicked()
                {
                    app.scan_bind_for_selected_client();
                }
            });
        });
    });

    ui.add_space(10.0);
    egui::CollapsingHeader::new("Withdraw Review").default_open(false).show(ui, |ui| {
        panel(ui, "Withdraw Review", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        client_ready && !app.is_busy,
                        egui::Button::new("Refresh pending orders"),
                    )
                    .clicked()
                {
                    app.fetch_pending_withdraw_orders_for_selected_client();
                }
                if ui.button("Select all").clicked() {
                    app.selected_withdraw_trade_nos = app
                        .pending_withdraw_orders
                        .iter()
                        .map(|order| order.trade_no.clone())
                        .collect();
                }
                if ui.button("Clear selection").clicked() {
                    app.selected_withdraw_trade_nos.clear();
                }
                ui.label(
                    RichText::new(format!(
                        "{} pending, {} selected",
                        app.pending_withdraw_orders.len(),
                        app.selected_withdraw_trade_nos.len()
                    ))
                    .color(Color32::from_rgb(105, 118, 132)),
                );
            });
            draw_withdraw_order_table(app, ui);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        client_ready && !app.is_busy,
                        egui::Button::new("Approve selected"),
                    )
                    .clicked()
                {
                    app.review_withdraw_orders_for_selected_client(true);
                }
                if ui
                    .add_enabled(client_ready && !app.is_busy, egui::Button::new("Reject selected"))
                    .clicked()
                {
                    app.review_withdraw_orders_for_selected_client(false);
                }
            });

            ui.separator();
            ui.label(RichText::new("Manual trade numbers fallback").strong());
            ui.add_sized(
                [ui.available_width(), 70.0],
                TextEdit::multiline(&mut app.withdraw_review_trade_nos)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("One trade_no per line, or comma separated"),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(client_ready && !app.is_busy, egui::Button::new("Approve orders"))
                    .clicked()
                {
                    app.review_withdraw_orders_for_selected_client(true);
                }
                if ui
                    .add_enabled(client_ready && !app.is_busy, egui::Button::new("Reject orders"))
                    .clicked()
                {
                    app.review_withdraw_orders_for_selected_client(false);
                }
            });
        });
    });

    ui.add_space(10.0);
    egui::CollapsingHeader::new("Batch Transfer").default_open(false).show(ui, |ui| {
        panel(ui, "Batch Transfer", |ui| {
            let can_query_accounts = client_ready
                && !app.is_busy
                && !app.transfer_form.sub_wallet_address.trim().is_empty()
                && !app.transfer_form.chain_code.trim().is_empty();
            two_column_fields(ui, |ui| {
                field(ui, "Chain", &mut app.transfer_form.chain_code);
                field(ui, "Amount", &mut app.transfer_form.value);
                field(ui, "From", &mut app.transfer_form.from_address);
                field(ui, "Symbol", &mut app.transfer_form.symbol);
                field(ui, "Subwallet", &mut app.transfer_form.sub_wallet_address);
                field(ui, "Decimals", &mut app.transfer_form.decimals);
                field(ui, "Concurrency", &mut app.transfer_form.max_in_flight);
                field(ui, "Interval ms", &mut app.transfer_form.start_interval_ms);
                field(ui, "Fee setting", &mut app.transfer_form.fee_setting);
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_query_accounts, egui::Button::new("Show account addresses"))
                    .clicked()
                {
                    app.refresh_account_addresses_for_selected_client();
                }
                if ui.button("Copy account details").clicked() {
                    ui.output_mut(|output| output.copied_text = account_details_text(app));
                }
                if ui.button("Copy addresses only").clicked() {
                    ui.output_mut(|output| output.copied_text = app.account_addresses_only.clone());
                }
                if ui.button("Use as targets").clicked() {
                    app.use_account_addresses_as_targets();
                }
            });

            ui.add_space(6.0);
            ui.label(RichText::new("Target addresses").strong());
            ui.add_sized(
                [ui.available_width(), 145.0],
                TextEdit::multiline(&mut app.transfer_form.to_addresses_raw)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("One target address per line"),
            );

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(client_ready && !app.is_busy, egui::Button::new("Load targets"))
                    .clicked()
                {
                    app.load_targets_for_selected_client();
                }
                if ui
                    .add_enabled(client_ready && !app.is_busy, egui::Button::new("Run transfer"))
                    .clicked()
                {
                    app.run_transfer_for_selected_client();
                }
                if app.is_busy {
                    ui.label(RichText::new("Running...").color(Color32::from_rgb(166, 116, 0)));
                }
            });

            if let Some(summary) = &app.last_summary {
                ui.label(
                    RichText::new(format!(
                        "Last summary: total={}, success={}, failed={}",
                        summary.total, summary.success, summary.failed
                    ))
                    .color(Color32::from_rgb(47, 90, 65)),
                );
            }
        });
    });
}

fn draw_subwallet_comma_copy_controls(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Subwallet comma copy").strong());
        ui.label("from account");
        ui.add_sized([72.0, 22.0], TextEdit::singleline(&mut app.comma_copy_start_account_id));
        ui.label("count");
        ui.add_sized([72.0, 22.0], TextEdit::singleline(&mut app.comma_copy_count));

        if ui.button("Copy comma addresses").clicked() {
            let text = subwallet_comma_addresses(app);
            ui.output_mut(|output| output.copied_text = text);
        }
    });
    ui.label(
        RichText::new("Copies loaded subwallet account addresses as address1,address2,address3")
            .color(Color32::from_rgb(105, 118, 132)),
    );
}

fn subwallet_comma_addresses(app: &ApiWalletConsoleApp) -> String {
    let start = app.comma_copy_start_account_id.trim().parse::<u32>().unwrap_or(1);
    let count = app.comma_copy_count.trim().parse::<usize>().unwrap_or(10);
    let Some(subwallet) = app.api_wallet_rows.iter().find(|wallet| wallet.role == "subwallet")
    else {
        return String::new();
    };

    let mut rows = app
        .account_address_rows
        .iter()
        .filter(|row| row.wallet_address == subwallet.address && row.account_id >= start)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.account_id);
    rows.into_iter().take(count).map(|row| row.address.as_str()).collect::<Vec<_>>().join(",")
}

fn draw_withdraw_order_table(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    if app.pending_withdraw_orders.is_empty() {
        ui.label(
            RichText::new("No pending orders loaded. Click Refresh pending orders.")
                .color(Color32::from_rgb(130, 140, 150)),
        );
        return;
    }

    ScrollArea::vertical()
        .id_source("pending_withdraw_orders")
        .max_height(240.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let orders = app.pending_withdraw_orders.clone();
            for order in orders {
                Frame::none()
                    .fill(Color32::from_rgb(248, 250, 252))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 234)))
                    .rounding(6.0)
                    .inner_margin(Margin::same(8.0))
                    .show(ui, |ui| {
                        let mut selected =
                            app.selected_withdraw_trade_nos.contains(&order.trade_no);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut selected, "").changed() {
                                if selected {
                                    app.selected_withdraw_trade_nos.insert(order.trade_no.clone());
                                } else {
                                    app.selected_withdraw_trade_nos.remove(&order.trade_no);
                                }
                            }
                            ui.label(RichText::new(order.trade_no.as_str()).monospace().strong());
                        });
                        ui.label(format!(
                            "{} {} on {} | status {}",
                            order.value, order.symbol, order.chain_code, order.status
                        ));
                        if let Some(out_order_id) = order.out_order_id.as_deref() {
                            ui.label(format!("out_order_id: {out_order_id}"));
                        }
                        if let Some(client_id) = order.client_id.as_deref() {
                            ui.label(format!("client_id: {client_id}"));
                        }
                        ui.label(RichText::new(format!("to: {}", order.to_addr)).monospace());
                    });
                ui.add_space(6.0);
            }
        });
}

fn draw_api_wallet_table(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    if app.api_wallet_rows.is_empty() {
        let message = if app.selected_client_ready() {
            "No API wallets loaded yet. Click Refresh wallets from DB, or import configured wallets first."
        } else {
            "Start this client to auto-load API wallets from its local DB."
        };
        ui.label(RichText::new(message).color(Color32::from_rgb(130, 140, 150)));
        return;
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new("API wallets from local DB").strong());
        ui.label(
            RichText::new(format!("{} rows", app.api_wallet_rows.len()))
                .color(Color32::from_rgb(105, 118, 132)),
        );
    });
    ScrollArea::horizontal()
        .id_source("api_wallet_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Grid::new("api_wallet_table")
                .striped(true)
                .num_columns(9)
                .spacing(Vec2::new(12.0, 4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Role").strong());
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Address").strong());
                    ui.label(RichText::new("UID").strong());
                    ui.label(RichText::new("Accounts").strong());
                    ui.label(RichText::new("Balances").strong());
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.end_row();

                    let wallets = app.api_wallet_rows.clone();
                    for wallet in wallets {
                        let account_count = app
                            .account_address_rows
                            .iter()
                            .filter(|row| row.wallet_address == wallet.address)
                            .count();
                        let balance_count = app
                            .balance_asset_rows
                            .iter()
                            .filter(|row| row.wallet_address == wallet.address)
                            .count();
                        ui.label(wallet.role.as_str());
                        ui.label(wallet.name.as_str());
                        ui.label(RichText::new(wallet.address.as_str()).monospace());
                        ui.label(RichText::new(wallet.uid.as_str()).monospace());
                        ui.label(account_count.to_string());
                        ui.label(balance_count.to_string());
                        if ui.button("Show accounts").clicked() {
                            app.refresh_account_addresses_for_wallet(
                                wallet.address.clone(),
                                app.transfer_form.chain_code.trim().to_string(),
                            );
                        }
                        if ui.button("Show balances").clicked() {
                            app.refresh_balances_for_wallet(wallet.address.clone());
                        }
                        if wallet.role == "subwallet" {
                            if ui.button("Use").clicked() {
                                app.transfer_form.sub_wallet_address = wallet.address.clone();
                                if let Some(slot) =
                                    app.transfer_forms.get_mut(app.selected_client.0)
                                {
                                    *slot = app.transfer_form.clone();
                                }
                            }
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        });
}

fn draw_runtime_info(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    let runtime = app.selected_client().and_then(|client| client.runtime_info.as_ref());
    Grid::new("runtime_info_table").num_columns(2).spacing(Vec2::new(14.0, 5.0)).show(ui, |ui| {
        ui.label(RichText::new("Device SN").strong());
        ui.label(
            RichText::new(runtime.map(|info| info.device_sn.as_str()).unwrap_or("not loaded yet"))
                .monospace(),
        );
        ui.end_row();

        ui.label(RichText::new("Device type").strong());
        ui.label(runtime.map(|info| info.device_type.as_str()).unwrap_or("-"));
        ui.end_row();

        ui.label(RichText::new("Device Init App ID").strong());
        ui.label(
            RichText::new(runtime.and_then(|info| info.device_app_id.as_deref()).unwrap_or("-"))
                .monospace(),
        );
        ui.end_row();

        ui.label(RichText::new("Package / Version").strong());
        let package = runtime.and_then(|info| info.package_id.as_deref()).unwrap_or("-");
        let version = runtime.map(|info| info.app_version.as_str()).unwrap_or("-");
        ui.label(format!("{package} / {version}"));
        ui.end_row();

        ui.label(RichText::new("Binding App ID").strong());
        ui.label(RichText::new(app.bind_form.app_id.as_str()).monospace());
        ui.end_row();

        ui.label(RichText::new("Bind Org ID").strong());
        ui.label(RichText::new(app.bind_form.org_id.as_str()).monospace());
        ui.end_row();

        ui.label(RichText::new("Sub UID").strong());
        ui.label(RichText::new(app.bind_form.subaccount_uid.as_str()).monospace());
        ui.end_row();

        ui.label(RichText::new("Withdraw UID").strong());
        ui.label(RichText::new(app.bind_form.withdrawal_uid.as_str()).monospace());
        ui.end_row();
    });
}

fn draw_messages_panel(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    if app.log_panel_widths.len() != app.clients.len() {
        app.log_panel_widths.resize(app.clients.len(), 340.0);
    }

    Frame::none()
        .fill(Color32::from_rgb(28, 35, 43))
        .rounding(6.0)
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Logs").size(18.0).color(Color32::WHITE).strong());
                if ui.button("Clear").clicked() {
                    app.logs.clear();
                }
                ui.separator();
                selectable_log_filter(ui, app, LogFilter::All, "All");
                selectable_log_filter(ui, app, LogFilter::Selected, "Selected");
                selectable_log_filter(ui, app, LogFilter::Notify, "Notify");
                selectable_log_filter(ui, app, LogFilter::Error, "Error");
                ui.label(
                    RichText::new(format!("{} lines", app.logs.len()))
                        .color(Color32::from_rgb(170, 184, 198)),
                );
            });

            ui.add_space(6.0);
            let height = (ui.available_height() - 4.0).max(90.0);
            let available_width = ui.available_width();
            normalize_log_panel_widths(app, available_width);
            ScrollArea::horizontal()
                .id_source("client_log_terminal_row")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let clients = app.clients.clone();
                        for (index, client) in clients.iter().enumerate() {
                            if index > 0 {
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::new(12.0, height), Sense::drag());
                                let handle_color = if response.dragged() || response.hovered() {
                                    Color32::from_rgb(120, 170, 205)
                                } else {
                                    Color32::from_rgb(64, 76, 88)
                                };
                                ui.painter().rect_filled(
                                    rect.shrink2(Vec2::new(4.0, 0.0)),
                                    2.0,
                                    handle_color,
                                );
                                if response.hovered() || response.dragged() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                                }
                                if response.dragged() {
                                    let delta = response.drag_delta().x;
                                    app.log_panel_widths[index - 1] =
                                        (app.log_panel_widths[index - 1] + delta).max(180.0);
                                    app.log_panel_widths[index] =
                                        (app.log_panel_widths[index] - delta).max(180.0);
                                }
                            }

                            let width = app
                                .log_panel_widths
                                .get(index)
                                .copied()
                                .unwrap_or(340.0)
                                .max(180.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(width, height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| draw_client_log_terminal(app, ui, client.name.as_str()),
                            );
                        }
                    });
                });
        });
}

fn normalize_log_panel_widths(app: &mut ApiWalletConsoleApp, available_width: f32) {
    let count = app.clients.len();
    if count == 0 {
        return;
    }
    if app.log_panel_widths.len() != count {
        app.log_panel_widths.resize(count, 0.0);
    }

    let handle_total = 12.0 * count.saturating_sub(1) as f32;
    let target_total = (available_width - handle_total - 8.0).max(180.0 * count as f32);
    let current_total: f32 = app.log_panel_widths.iter().sum();
    if current_total <= 0.0 {
        let width = target_total / count as f32;
        for panel_width in &mut app.log_panel_widths {
            *panel_width = width.max(180.0);
        }
        return;
    }

    if (target_total - current_total).abs() > 2.0 {
        let scale = target_total / current_total;
        for panel_width in &mut app.log_panel_widths {
            *panel_width = (*panel_width * scale).max(180.0);
        }
    }
}

fn draw_client_log_terminal(app: &ApiWalletConsoleApp, ui: &mut egui::Ui, client_name: &str) {
    Frame::none()
        .fill(Color32::from_rgb(18, 25, 32))
        .stroke(Stroke::new(1.0, Color32::from_rgb(55, 67, 79)))
        .rounding(6.0)
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            let lines = filtered_logs_for_client(app, client_name);
            let mqtt_count = app
                .logs
                .iter()
                .filter(|line| {
                    line.starts_with(&format!("{client_name}:"))
                        && (line.contains(" notify:") || line.contains("MQTT_"))
                })
                .count();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(client_name).color(Color32::from_rgb(235, 242, 248)).strong(),
                );
                ui.label(
                    RichText::new(format!("{} lines", lines.len()))
                        .color(Color32::from_rgb(145, 160, 174)),
                );
                ui.label(
                    RichText::new(format!("mqtt {mqtt_count}"))
                        .color(Color32::from_rgb(145, 220, 170)),
                );
            });
            ui.add_space(4.0);
            ScrollArea::vertical()
                .id_source(format!("client_log_terminal_{client_name}"))
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in lines {
                        ui.label(
                            RichText::new(line.as_str())
                                .monospace()
                                .color(Color32::from_rgb(215, 224, 232)),
                        );
                    }
                    if filtered_logs_for_client(app, client_name).is_empty() {
                        ui.label(
                            RichText::new("No logs yet")
                                .monospace()
                                .color(Color32::from_rgb(105, 118, 132)),
                        );
                    }
                })
        });
}

fn draw_transfer_confirm_dialog(app: &mut ApiWalletConsoleApp, ctx: &egui::Context) {
    if !app.show_transfer_confirm {
        return;
    }

    let target_count =
        app.transfer_form.to_addresses_raw.lines().filter(|line| !line.trim().is_empty()).count();
    let selected_name = app
        .selected_client()
        .map(|client| client.name.clone())
        .unwrap_or_else(|| "none".to_string());

    egui::Window::new("Confirm batch transfer").collapsible(false).resizable(false).show(
        ctx,
        |ui| {
            ui.label(RichText::new("Review before submitting").strong());
            ui.add_space(8.0);
            ui.label(format!("Client: {selected_name}"));
            ui.label(format!("Chain: {}", app.transfer_form.chain_code));
            ui.label(format!("From: {}", app.transfer_form.from_address));
            ui.label(format!("Amount per target: {}", app.transfer_form.value));
            ui.label(format!("Target count: {target_count}"));
            ui.label(format!("Concurrency: {}", app.transfer_form.max_in_flight));
            ui.label(format!("Interval ms: {}", app.transfer_form.start_interval_ms));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    app.cancel_run_transfer();
                }
                if ui.button("Confirm and run").clicked() {
                    app.confirm_run_transfer_for_selected_client();
                }
            });
        },
    );
}

fn selectable_log_filter(
    ui: &mut egui::Ui,
    app: &mut ApiWalletConsoleApp,
    filter: LogFilter,
    label: &str,
) {
    if ui.selectable_label(app.log_filter == filter, label).clicked() {
        app.log_filter = filter;
    }
}

fn filtered_logs_for_client<'a>(
    app: &'a ApiWalletConsoleApp,
    client_name: &str,
) -> Vec<&'a String> {
    let client_prefix = format!("{client_name}:");
    let selected_prefix = app.selected_client().map(|client| format!("{}:", client.name));
    app.logs
        .iter()
        .filter(|line| line.starts_with(&client_prefix))
        .filter(|line| match app.log_filter {
            LogFilter::All => true,
            LogFilter::Selected => {
                selected_prefix.as_ref().map(|prefix| line.starts_with(prefix)).unwrap_or(false)
            }
            LogFilter::Notify => line.contains(" notify:"),
            LogFilter::Error => {
                let lower = line.to_lowercase();
                lower.contains("failed") || lower.contains("error")
            }
        })
        .collect()
}

fn draw_account_address_table(app: &mut ApiWalletConsoleApp, ui: &mut egui::Ui) {
    Frame::none()
        .fill(Color32::from_rgb(248, 250, 252))
        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 234)))
        .rounding(6.0)
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            if app.account_address_rows.is_empty() {
                ui.label(
                    RichText::new(app.account_address_status.as_str())
                        .color(Color32::from_rgb(130, 80, 40)),
                );
                return;
            }

            ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                Grid::new("account_address_table")
                    .striped(true)
                    .num_columns(5)
                    .spacing(Vec2::new(14.0, 4.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("ID").strong());
                        ui.label(RichText::new("Name").strong());
                        ui.label(RichText::new("Chain").strong());
                        ui.label(RichText::new("Address").strong());
                        ui.label("");
                        ui.end_row();

                        for row in &app.account_address_rows {
                            ui.label(row.account_id.to_string());
                            ui.label(row.name.as_str());
                            ui.label(row.chain_code.as_str());
                            ui.label(RichText::new(row.address.as_str()).monospace());
                            if ui.button("Copy").clicked() {
                                ui.output_mut(|output| output.copied_text = row.address.clone());
                            }
                            ui.end_row();
                        }
                    });
            });
        });
}

fn draw_account_address_table_for_wallet(
    app: &mut ApiWalletConsoleApp,
    ui: &mut egui::Ui,
    wallet: &super::events::ApiWalletRow,
) {
    let rows = app
        .account_address_rows
        .iter()
        .filter(|row| row.wallet_address == wallet.address)
        .cloned()
        .collect::<Vec<_>>();

    Frame::none()
        .fill(Color32::from_rgb(248, 250, 252))
        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 234)))
        .rounding(6.0)
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(wallet.role.as_str()).strong());
                ui.label(wallet.name.as_str());
                ui.label(RichText::new(wallet.address.as_str()).monospace());
                ui.label(
                    RichText::new(format!("{} accounts", rows.len()))
                        .color(Color32::from_rgb(105, 118, 132)),
                );
                if ui.button("Show").clicked() {
                    app.refresh_account_addresses_for_wallet(
                        wallet.address.clone(),
                        app.transfer_form.chain_code.trim().to_string(),
                    );
                }
                if ui.button("Copy details").clicked() {
                    ui.output_mut(|output| output.copied_text = account_details_rows_text(&rows));
                }
                if ui.button("Copy addresses").clicked() {
                    ui.output_mut(|output| {
                        output.copied_text = rows
                            .iter()
                            .map(|row| row.address.as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    });
                }
            });

            if rows.is_empty() {
                ui.label(
                    RichText::new("No account addresses loaded for this wallet yet.")
                        .color(Color32::from_rgb(130, 140, 150)),
                );
                return;
            }

            ui.add_space(4.0);
            egui::Resize::default()
                .id_source(format!("account_address_wallet_resize_{}", wallet.address))
                .default_width(ui.available_width())
                .default_height(320.0)
                .min_height(120.0)
                .max_width(ui.available_width())
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_source(format!("account_address_scroll_{}", wallet.address))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            Grid::new(format!("account_address_table_{}", wallet.address))
                                .striped(true)
                                .num_columns(5)
                                .spacing(Vec2::new(14.0, 4.0))
                                .show(ui, |ui| {
                                    for row in rows.iter() {
                                        ui.label(row.account_id.to_string());
                                        ui.label(row.name.as_str());
                                        ui.label(row.chain_code.as_str());
                                        ui.label(RichText::new(row.address.as_str()).monospace());
                                        if ui.button("Copy").clicked() {
                                            ui.output_mut(|output| {
                                                output.copied_text = row.address.clone()
                                            });
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                });
        });
}

fn draw_balance_table_for_wallet(
    app: &mut ApiWalletConsoleApp,
    ui: &mut egui::Ui,
    wallet: &super::events::ApiWalletRow,
) {
    let rows = app
        .balance_asset_rows
        .iter()
        .filter(|row| row.wallet_address == wallet.address)
        .cloned()
        .collect::<Vec<_>>();

    Frame::none()
        .fill(Color32::from_rgb(248, 250, 252))
        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 234)))
        .rounding(6.0)
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(wallet.role.as_str()).strong());
                ui.label(wallet.name.as_str());
                ui.label(RichText::new(wallet.address.as_str()).monospace());
                ui.label(
                    RichText::new(format!("{} assets", rows.len()))
                        .color(Color32::from_rgb(105, 118, 132)),
                );
                if ui.button("Show").clicked() {
                    app.refresh_balances_for_wallet(wallet.address.clone());
                }
            });

            if rows.is_empty() {
                ui.label(
                    RichText::new("No balances loaded for this wallet yet.")
                        .color(Color32::from_rgb(130, 140, 150)),
                );
                return;
            }

            Grid::new(format!("balance_table_{}", wallet.address))
                .striped(true)
                .num_columns(5)
                .spacing(Vec2::new(14.0, 4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Chain").strong());
                    ui.label(RichText::new("Symbol").strong());
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Amount").strong());
                    ui.label(RichText::new("Fiat").strong());
                    ui.end_row();

                    for row in rows.iter() {
                        ui.label(row.chain_code.as_str());
                        ui.label(row.symbol.as_str());
                        ui.label(row.name.as_str());
                        ui.label(RichText::new(format!("{:.8}", row.amount)).monospace());
                        ui.label(
                            row.fiat_value
                                .map(|value| format!("{value:.4} {}", row.currency))
                                .unwrap_or_else(|| "-".to_string()),
                        );
                        ui.end_row();
                    }
                });
        });
}

fn account_details_text(app: &ApiWalletConsoleApp) -> String {
    account_details_rows_text(&app.account_address_rows)
}

fn account_details_rows_text(rows: &[super::events::AccountAddressRow]) -> String {
    rows.iter()
        .map(|row| {
            format!(
                "wallet={} account_id={} name={} chain={} address={}",
                row.wallet_address, row.account_id, row.name, row.chain_code, row.address
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn panel(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .fill(Color32::from_rgb(255, 255, 255))
        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 234)))
        .rounding(8.0)
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(17.0).strong().color(Color32::from_rgb(38, 50, 61)));
            ui.add_space(8.0);
            add_contents(ui);
        });
}

fn two_column_fields(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(10.0, 6.0);
        add_contents(ui);
    });
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width().min(760.0));
        ui.add_sized([96.0, 22.0], egui::Label::new(label));
        ui.add_sized([ui.available_width().max(220.0), 22.0], TextEdit::singleline(value));
    });
}

fn small_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [62.0, 20.0],
            egui::Label::new(RichText::new(label).color(Color32::from_rgb(92, 105, 118))),
        );
        ui.add_sized([ui.available_width(), 20.0], TextEdit::singleline(value));
    });
}

fn status_badge(status: &ClientStatus) -> RichText {
    match status {
        ClientStatus::Idle => RichText::new("idle").color(Color32::from_rgb(105, 116, 126)),
        ClientStatus::Starting => RichText::new("starting").color(Color32::from_rgb(166, 116, 0)),
        ClientStatus::Ready => {
            RichText::new("ready").color(Color32::from_rgb(22, 128, 78)).strong()
        }
        ClientStatus::Failed => {
            RichText::new("failed").color(Color32::from_rgb(190, 45, 45)).strong()
        }
    }
}
