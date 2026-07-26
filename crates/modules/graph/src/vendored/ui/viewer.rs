use egui::{Painter, Style};
use mara_core::MaraUi;
use mara_core::vocab::{Pos2, Rect};

use crate::vendored::{Graph, InPin, InPinId, NodeId, OutPin, OutPinId};

use super::{
    BackgroundPattern, GraphStyle, NodeLayout,
    pin::{AnyPins, NodePin},
};

/// `NodeViewer` is a trait for viewing a Graph.
///
/// It can extract necessary data from the nodes and controls their
/// response to certain events.
pub trait NodeViewer<T> {
    /// Returns title of the node.
    fn title(&mut self, node: &T) -> String;

    /// Returns the node's frame.
    /// All node's elements will be rendered inside this frame.
    /// Except for pins if they are configured to be rendered outside of the frame.
    ///
    /// Returns `default` by default.
    /// `default` frame is taken from the [`GraphStyle::node_frame`] or constructed if it's `None`.
    ///
    /// Override this method to customize the frame for specific nodes.
    fn node_frame(
        &mut self,
        default: mara_core::style::FrameSpec,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        graph: &Graph<T>,
    ) -> mara_core::style::FrameSpec {
        let _ = (node, inputs, outputs, graph);
        default
    }

    /// Returns the node's header frame.
    ///
    /// This frame would be placed on top of the node's frame.
    /// And header UI (see [`show_header`]) will be placed inside this frame.
    ///
    /// Returns `default` by default.
    /// `default` frame is taken from the [`GraphStyle::header_frame`],
    /// or [`GraphStyle::node_frame`] with removed shadow if `None`,
    /// or constructed if both are `None`.
    fn header_frame(
        &mut self,
        default: mara_core::style::FrameSpec,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        graph: &Graph<T>,
    ) -> mara_core::style::FrameSpec {
        let _ = (node, inputs, outputs, graph);
        default
    }
    /// Checks if node has a custom egui style.
    #[inline]
    fn has_node_style(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        graph: &Graph<T>,
    ) -> bool {
        let _ = (node, inputs, outputs, graph);
        false
    }

    /// Modifies the node's egui style
    fn apply_node_style(
        &mut self,
        style: &mut Style,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        graph: &Graph<T>,
    ) {
        let _ = (style, node, inputs, outputs, graph);
    }

    /// Returns elements layout for the node.
    ///
    /// Node consists of 5 parts: header, body, footer, input pins and output pins.
    /// See [`NodeLayout`] for available placements.
    ///
    /// Returns `default` by default.
    /// `default` layout is taken from the [`GraphStyle::node_layout`] or constructed if it's `None`.
    /// Override this method to customize the layout for specific nodes.
    #[inline]
    fn node_layout(
        &mut self,
        default: NodeLayout,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        graph: &Graph<T>,
    ) -> NodeLayout {
        let _ = (node, inputs, outputs, graph);
        default
    }

    /// Renders elements inside the node's header frame.
    ///
    /// This is the good place to show the node's title and controls related to the whole node.
    ///
    /// By default it shows the node's title.
    #[inline]
    fn show_header(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (inputs, outputs);
        ui.label(&self.title(&graph[node]));
    }

    /// Returns number of input pins of the node.
    ///
    /// [`NodeViewer::show_input`] will be called for each input in range `0..inputs()`.
    fn inputs(&mut self, node: &T) -> usize;

    /// Renders one specified node's input element and returns drawer for the corresponding pin.
    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) -> impl NodePin + 'static;

    /// Returns number of output pins of the node.
    ///
    /// [`NodeViewer::show_output`] will be called for each output in range `0..outputs()`.
    fn outputs(&mut self, node: &T) -> usize;

    /// Renders the node's output.
    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) -> impl NodePin + 'static;

    /// Checks if node has something to show in body - between input and output pins.
    #[inline]
    fn has_body(&mut self, node: &T) -> bool {
        let _ = node;
        false
    }

    /// Renders the node's body.
    #[inline]
    fn show_body(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (node, inputs, outputs, ui, graph);
    }

    /// Checks if node has something to show in footer - below pins and body.
    #[inline]
    fn has_footer(&mut self, node: &T) -> bool {
        let _ = node;
        false
    }

    /// Renders the node's footer.
    #[inline]
    fn show_footer(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (node, inputs, outputs, ui, graph);
    }

    /// Reports the final node's rect after rendering.
    ///
    /// It aimed to be used for custom positioning of nodes that requires node dimensions for calculations.
    /// Node's position can be modified directly in this method.
    #[inline]
    fn final_node_rect(
        &mut self,
        node: NodeId,
        rect: Rect,
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (node, rect, ui, graph);
    }

    /// Checks if node has something to show in on-hover popup.
    #[inline]
    fn has_on_hover_popup(&mut self, node: &T) -> bool {
        let _ = node;
        false
    }

    /// Renders the node's on-hover popup.
    #[inline]
    fn show_on_hover_popup(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (node, inputs, outputs, ui, graph);
    }

    /// Checks if wire has something to show in widget.
    /// This may not be called if wire is invisible.
    #[inline]
    fn has_wire_widget(&mut self, from: &OutPinId, to: &InPinId, graph: &Graph<T>) -> bool {
        let _ = (from, to, graph);
        false
    }

    /// Renders the wire's widget.
    /// This may not be called if wire is invisible.
    #[inline]
    fn show_wire_widget(
        &mut self,
        from: &OutPin,
        to: &InPin,
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (from, to, ui, graph);
    }

    /// Checks if the graph has something to show in context menu if right-clicked or long-touched on empty space at `pos`.
    #[inline]
    fn has_graph_menu(&mut self, pos: Pos2, graph: &mut Graph<T>) -> bool {
        let _ = (pos, graph);
        false
    }

    /// Show context menu for the graph.
    ///
    /// This can be used to implement menu for adding new nodes.
    #[inline]
    fn show_graph_menu(&mut self, pos: Pos2, ui: &mut MaraUi<'_>, graph: &mut Graph<T>) {
        let _ = (pos, ui, graph);
    }

    /// Checks if the graph has something to show in context menu if wire drag is stopped at `pos`.
    #[inline]
    fn has_dropped_wire_menu(&mut self, src_pins: AnyPins, graph: &mut Graph<T>) -> bool {
        let _ = (src_pins, graph);
        false
    }

    /// Show context menu for the graph. This menu is opened when releasing a pin to empty
    /// space. It can be used to implement menu for adding new node, and directly
    /// connecting it to the released wire.
    #[inline]
    fn show_dropped_wire_menu(
        &mut self,
        pos: Pos2,
        ui: &mut MaraUi<'_>,
        src_pins: AnyPins,
        graph: &mut Graph<T>,
    ) {
        let _ = (pos, ui, src_pins, graph);
    }

    /// Checks if the node has something to show in context menu if right-clicked or long-touched on the node.
    #[inline]
    fn has_node_menu(&mut self, node: &T) -> bool {
        let _ = node;
        false
    }

    /// Show context menu for the graph.
    ///
    /// This can be used to implement menu for adding new nodes.
    #[inline]
    fn show_node_menu(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (node, inputs, outputs, ui, graph);
    }

    /// Asks the viewer to connect two pins.
    ///
    /// This is usually happens when user drags a wire from one node's output pin to another node's input pin or vice versa.
    /// By default this method connects the pins and returns `Ok(())`.
    #[inline]
    fn connect(&mut self, from: &OutPin, to: &InPin, graph: &mut Graph<T>) {
        graph.connect(from.id, to.id);
    }

    /// Asks the viewer to disconnect two pins.
    #[inline]
    fn disconnect(&mut self, from: &OutPin, to: &InPin, graph: &mut Graph<T>) {
        graph.disconnect(from.id, to.id);
    }

    /// Asks the viewer to disconnect all wires from the output pin.
    ///
    /// This is usually happens when right-clicking on an output pin.
    /// By default this method disconnects the pins and returns `Ok(())`.
    #[inline]
    fn drop_outputs(&mut self, pin: &OutPin, graph: &mut Graph<T>) {
        graph.drop_outputs(pin.id);
    }

    /// Asks the viewer to disconnect all wires from the input pin.
    ///
    /// This is usually happens when right-clicking on an input pin.
    /// By default this method disconnects the pins and returns `Ok(())`.
    #[inline]
    fn drop_inputs(&mut self, pin: &InPin, graph: &mut Graph<T>) {
        graph.drop_inputs(pin.id);
    }

    /// Draws background of the graph view.
    ///
    /// By default it draws the background pattern using [`BackgroundPattern::draw`].
    ///
    /// If you want to draw the background yourself, you can override this method.
    #[inline]
    fn draw_background(
        &mut self,
        background: Option<&BackgroundPattern>,
        viewport: &Rect,
        graph_style: &GraphStyle,
        style: &Style,
        painter: &Painter,
        graph: &Graph<T>,
    ) {
        let _ = graph;

        if let Some(background) = background {
            // The background pattern is fully ported to `MaraPainter`
            // (WS-D1.3); the surrounding renderer is not yet, so the
            // stroke resolves and the painter wraps here, at the seam
            // that shrinks as the rest of the port lands.
            let stroke = graph_style.get_bg_pattern_stroke(style);
            background.draw(
                &(*viewport).into(),
                mara_core::vocab::Stroke::new(
                    stroke.width,
                    mara_core::vocab::Color32::from(stroke.color),
                ),
                &mara_core::MaraPainter::__internal_from_egui(painter.clone()),
            );
        }
    }

    /// Informs the viewer what is the current transform of the graph view
    /// and allows viewer to override it.
    ///
    /// This method is called in the beginning of the graph rendering.
    ///
    /// By default it does nothing.
    #[inline]
    fn current_transform(
        &mut self,
        to_global: &mut mara_core::transform::Transform,
        graph: &mut Graph<T>,
    ) {
        let _ = (to_global, graph);
    }
}
