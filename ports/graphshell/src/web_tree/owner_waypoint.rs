// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Optional passive owner markers. No persistence, input or scheduling action.
use super::*;
use serde_json::{Value,json};
const ATTR:&str="data-graphshell-owner-waypoint";
const MAX_BYTES:usize=4096;
fn clock()->Option<(f64,f64)> {
    let p=web_sys::window()?.performance()?;
    let origin=p.time_origin();let now=p.now();
    (origin.is_finite() && now.is_finite()).then_some((origin,now))
}
fn read(root:&Element)->Option<Value> {
    let text=root.get_attribute(ATTR)?;
    if text.len()>MAX_BYTES { return None; }
    serde_json::from_str(&text).ok()
}
fn write(root:&Element,v:&Value) {
    let Ok(text)=serde_json::to_string(v) else { return; };
    if text.len()<=MAX_BYTES { let _=root.set_attribute(ATTR,&text); }
}
pub(super) fn configure(root:&Element) {
    let Some(window)=web_sys::window() else { return; };
    let Ok(search)=window.location().search() else { return; };
    let Ok(params)=web_sys::UrlSearchParams::new_with_str(&search) else { return; };
    if params.get("owner_waypoint").as_deref()!=Some("1") { return; }
    let Some((origin,_))=clock() else { return; };
    write(root,&json!({"schema":"graphshell.owner-waypoint/v1",
        "document_time_origin":origin,"ready":null,"pre_open":null,"import_saved":null}));
}
pub(super) struct OpenContext { root:Element, view:Value, model:Option<uuid::Uuid> }
pub(super) fn context(page:&TreePage)->Option<OpenContext> {
    // No query marker means no additional model/canvas reads.
    let _=read(&page.root)?;
    let product=page.product.as_ref()?;let pane=page.forme.as_ref()?;
    let canvas=page.shared.canvas.try_borrow().ok()?;
    let members=pane.model().members();let accesses=members.len();
    if accesses>2 { return None; }
    let region=canvas.forme_region();let cells=region.map_or(0,|r|r.cells.len());
    if cells>2 { return None; }
    let view=json!({"model_owner":pane.model().document.graph_id,"members":members,
        "accesses":accesses,"cells":cells,
        "model_document":pane.model().document.id.as_uuid(),
        "region_id":region.map(|r|r.id),
        "cell_members":region.map(|r|r.cells.iter().map(|c|c.member).collect::<Vec<_>>()).unwrap_or_default(),
        "forme_saving":pane.saving,
        "forme_save_succeeded":pane.status=="Workbench saved",
        "product_saving":product.saving,"product_save_state":product.save_state});
    Some(OpenContext {root:page.root.clone(),view,model:Some(pane.model().document.graph_id)})
}
pub(super) fn ready(page:&TreePage) {
    import_saved(page);
    let Some(mut marker)=read(&page.root) else { return; };
    if !marker["ready"].is_null() || !marker["pre_open"].is_null() { return; }
    let Some(ctx)=context(page) else { return; };
    let actual=page.product.as_ref().and_then(|p|p.actual_session());
    if actual.is_none() || actual!=ctx.model || ctx.view["accesses"]!=2
        || ctx.view["cells"]!=2 || ctx.view["forme_saving"]!=false
        || ctx.view["forme_save_succeeded"]!=true || ctx.view["product_saving"]!=false { return; }
    let Some((origin,at))=clock() else { return; };
    marker["ready"]=json!({"phase":"two_member_save_observed","document_time_origin":origin,
        "at_ms":at,"actual_app_present":true,"actual_owner":actual,
        "model_matches":true,"view":ctx.view});
    write(&ctx.root,&marker);
}
pub(super) fn pre_open(ctx:Option<&OpenContext>,actual:uuid::Uuid) {
    let Some(ctx)=ctx else { return; };
    let Some(mut marker)=read(&ctx.root) else { return; };
    // At most one exact synchronous Open marker in this diagnostic document.
    if !marker["pre_open"].is_null() { return; }
    let Some((origin,at))=clock() else { return; };
    marker["pre_open"]=json!({"phase":"immediately_before_codicil_replace",
        "document_time_origin":origin,"at_ms":at,"actual_app_present":true,
        "actual_owner":actual,"model_matches":ctx.model==Some(actual),"view":ctx.view});
    write(&ctx.root,&marker);
}

fn import_saved(page:&TreePage) {
    let Some(mut marker)=read(&page.root) else { return; };
    if marker["pre_open"].is_null() || !marker["import_saved"].is_null() { return; }
    let Some(product)=page.product.as_ref() else { return; };
    // These flags/status follow the actual operation Result consumed by Product.poll.
    if product.saving || product.save_state!="saved" || product.status!="Opened codicil" { return; }
    let Some(actual)=product.actual_session() else { return; };
    if marker["pre_open"]["actual_owner"]==json!(actual) || product.observed_session_changes!=1 { return; }
    let Some(ctx)=context(page) else { return; };
    let Ok(canvas)=page.shared.canvas.try_borrow() else { return; };
    let Some((origin,at))=clock() else { return; };
    marker["import_saved"]=json!({"phase":"actual_import_persist_result_consumed",
        "document_time_origin":origin,"at_ms":at,"actual_app_present":true,
        "actual_owner":actual,"actual_session_changes":product.observed_session_changes,
        "actual_graph_nodes":product.actual_graph_nodes(),
        "actual_canvas_nodes":canvas.graph().nodes().count(),"product_saving":product.saving,
        "save_state":product.save_state,"persist_success":true,"view":ctx.view});
    write(&ctx.root,&marker);
}
