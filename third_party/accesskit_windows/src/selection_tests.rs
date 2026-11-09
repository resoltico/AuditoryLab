// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! UIA arrays contain contract-specific interface pointers, not COM identities.
use crate::{
    context::{ActionHandlerNoMut, Context},
    util::safe_array_from_com_slice,
};
use accesskit::{
    ActionRequest, Node, NodeId, Role, TextPosition, TextSelection, TreeId, TreeUpdate,
};
use accesskit_consumer::Tree;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use windows::{
    Win32::{Foundation::HWND, System::Ole::*, UI::Accessibility::*},
    core::*,
};

struct NoActions;
impl ActionHandlerNoMut for NoActions {
    fn do_action(&self, _: ActionRequest) {}
}
fn context(nodes: Vec<(NodeId, Node)>, focus: NodeId) -> Arc<Context> {
    let tree = Tree::new(
        TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus,
        },
        true,
    );
    Context::new(HWND::default().into(), tree, Arc::new(NoActions), false)
}
fn consumer_id(context: &Context, local: NodeId) -> accesskit_consumer::NodeId {
    context
        .read_tree()
        .state()
        .node_by_tree_local_id(local, TreeId::ROOT)
        .unwrap()
        .id()
}
fn element(array: *mut windows::Win32::System::Com::SAFEARRAY) -> IUnknown {
    assert!(!array.is_null());
    let lower = unsafe { SafeArrayGetLBound(array, 1).unwrap() };
    let upper = unsafe { SafeArrayGetUBound(array, 1).unwrap() };
    assert_eq!((lower, upper), (0, 0));
    let mut raw = std::ptr::null_mut();
    // SafeArrayGetElement AddRefs the returned interface. The wrapper owns it.
    unsafe {
        SafeArrayGetElement(array, &0, &mut raw as *mut _ as *mut _).unwrap();
        IUnknown::from_raw(raw)
    }
}

#[test]
fn text_selection_array_preserves_the_text_range_interface() {
    let mut root = Node::new(Role::Window);
    root.set_children([NodeId(1)]);
    let mut input = Node::new(Role::MultilineTextInput);
    input.set_children([NodeId(2)]);
    input.set_text_selection(TextSelection {
        anchor: TextPosition {
            node: NodeId(2),
            character_index: 0,
        },
        focus: TextPosition {
            node: NodeId(2),
            character_index: 5,
        },
    });
    let mut text = Node::new(Role::TextRun);
    text.set_value("hello");
    text.set_character_lengths([1, 1, 1, 1, 1]);
    let context = context(
        vec![(NodeId(0), root), (NodeId(1), input), (NodeId(2), text)],
        NodeId(1),
    );
    let provider = context
        .get_or_create_platform_node(consumer_id(&context, NodeId(1)))
        .into_interface::<ITextProvider>();
    // This ordinary typed return is the independent positive control.
    assert_eq!(
        unsafe { provider.DocumentRange().unwrap().GetText(-1).unwrap() }.to_string(),
        "hello"
    );
    let array = unsafe { provider.GetSelection().unwrap() };
    assert_eq!(
        unsafe { SafeArrayGetIID(array).unwrap() },
        ITextRangeProvider::IID
    );
    let stored = element(array);
    // Check raw pointer *before* QueryInterface could repair a canonical identity.
    let identity = stored.cast::<IUnknown>().unwrap();
    assert_ne!(
        stored.as_raw(),
        identity.as_raw(),
        "UIA needs the text-range vtable, not the separate identity vtable"
    );
    let range = stored.cast::<ITextRangeProvider>().unwrap();
    assert_eq!(stored.as_raw(), range.as_raw());
    assert_eq!(unsafe { range.GetText(-1).unwrap() }.to_string(), "hello");
    unsafe {
        SafeArrayDestroy(array).unwrap();
    }
    // The returned reference remains usable after the array releases its own.
    assert_eq!(unsafe { range.GetText(-1).unwrap() }.to_string(), "hello");
}

#[test]
fn item_selection_array_preserves_the_element_provider_interface() {
    let mut root = Node::new(Role::Window);
    root.set_children([NodeId(1)]);
    let mut list = Node::new(Role::ListBox);
    list.set_children([NodeId(2)]);
    list.set_controls([NodeId(2)]);
    let mut item = Node::new(Role::ListBoxOption);
    item.set_selected(true);
    item.set_label("selected item");
    let context = context(
        vec![(NodeId(0), root), (NodeId(1), list), (NodeId(2), item)],
        NodeId(2),
    );
    let expected = context
        .get_or_create_platform_node(consumer_id(&context, NodeId(2)))
        .into_interface::<IRawElementProviderSimple>();
    let provider = context
        .get_or_create_platform_node(consumer_id(&context, NodeId(1)))
        .into_interface::<ISelectionProvider>();
    let array = unsafe { provider.GetSelection().unwrap() };
    assert_eq!(
        unsafe { SafeArrayGetIID(array).unwrap() },
        IRawElementProviderSimple::IID
    );
    let stored = element(array);
    assert_eq!(
        stored.as_raw(),
        expected.as_raw(),
        "UIA needs the element-provider vtable, not canonical IUnknown"
    );
    assert_ne!(
        stored.as_raw(),
        expected.cast::<IUnknown>().unwrap().as_raw()
    );
    let returned = stored.cast::<IRawElementProviderSimple>().unwrap();
    assert_eq!(returned.as_raw(), stored.as_raw());
    assert_eq!(
        unsafe { returned.ProviderOptions().unwrap() },
        ProviderOptions_ServerSideProvider
    );
    unsafe {
        SafeArrayDestroy(array).unwrap();
    }
    // The related ControllerFor getter must preserve the same typed element
    // pointer; its property-change path already uses inheritance conversion.
    let list = context
        .get_or_create_platform_node(consumer_id(&context, NodeId(1)))
        .into_interface::<IRawElementProviderSimple>();
    let controls = unsafe { list.GetPropertyValue(UIA_ControllerForPropertyId).unwrap() };
    assert_eq!(
        controls.vt(),
        windows::Win32::System::Variant::VT_ARRAY | windows::Win32::System::Variant::VT_UNKNOWN
    );
    let array = unsafe { controls.Anonymous.Anonymous.Anonymous.parray };
    assert_eq!(
        unsafe { SafeArrayGetIID(array).unwrap() },
        IRawElementProviderSimple::IID
    );
    let stored = element(array);
    assert_eq!(stored.as_raw(), expected.as_raw());
    // VARIANT owns and destroys this property array.
}

#[implement(IValueProvider)]
struct DropProbe(Arc<AtomicUsize>);
impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl IValueProvider_Impl for DropProbe_Impl {
    fn SetValue(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn Value(&self) -> Result<BSTR> {
        Ok(BSTR::from("live array reference"))
    }
    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(true.into())
    }
}
#[test]
fn typed_interface_array_owns_exactly_one_additional_reference() {
    let drops = Arc::new(AtomicUsize::new(0));
    let provider: IValueProvider = DropProbe(drops.clone()).into();
    let array = safe_array_from_com_slice(std::slice::from_ref(&provider));
    assert_eq!(
        unsafe { SafeArrayGetIID(array).unwrap() },
        IValueProvider::IID
    );
    let stored = element(array);
    assert_eq!(stored.as_raw(), provider.as_raw());
    drop(stored);
    drop(provider);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        0,
        "SAFEARRAY must retain its own reference"
    );
    unsafe {
        SafeArrayDestroy(array).unwrap();
    }
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "SAFEARRAY destruction must release its reference"
    );
}
