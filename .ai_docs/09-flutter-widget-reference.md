# Flutter Widget Reference

A comprehensive catalogue of the widgets (and the essential supporting classes)
in the Flutter SDK (`package:flutter`), organised by category. For each entry:

- **Type** — the widget's base class / kind (`StatelessWidget`, `StatefulWidget`,
  `RenderObjectWidget`, `InheritedWidget`, delegate, function, etc.).
- **Capabilities** — what it does and when to use it.
- **Key constructors / named constructors** — the constructors and their
  distinguishing parameters.
- **Key properties (constructor args)** — the important properties.
- **Key methods** — notable public / overridable methods. Most Flutter widgets
  are immutable configuration objects whose primary method is `build`
  (composition widgets) or `createRenderObject` / `updateRenderObject`
  (render-object widgets); `StatefulWidget`s expose `createState` and their
  `State` carries the lifecycle (`initState`, `didChangeDependencies`,
  `didUpdateWidget`, `build`, `deactivate`, `dispose`).

> Note: Flutter widgets are overwhelmingly immutable. The "methods" of a typical
> widget are few — the configuration lives in constructor parameters. Where a
> widget has a meaningful imperative surface (e.g. `FormState`, `TabController`,
> `ScaffoldMessengerState`, `AnimationController`), those methods are listed.

## Table of contents

1. [Core / foundational classes](#a-core--foundational-widget-classes)
2. [Basic single-child layout & boxes](#b-basic-single-child-layout--boxes)
3. [Multi-child layout & flex](#multi-child-layout--flex)
4. [Material — app structure, scaffolding, navigation & tiles](#material--app-structure-scaffolding-navigation--tiles)
5. [Material — buttons, selection, dialogs & feedback](#material--buttons-selection-dialogs--feedback)
6. [Text & input](#text--input)
7. [Scrolling & slivers](#scrolling--slivers)
8. [Painting, effects, images & icons](#painting-effects-images--icons)
9. [Animation & motion](#animation--motion)
10. [Gestures, interaction, async & accessibility](#gestures-interaction-async--accessibility)
11. [Cupertino (iOS-style)](#cupertino-ios-style)
12. [Inherited / utility / app-config widgets](#inherited--utility--app-config-widgets)

---

## A. Core / Foundational widget classes

### Widget
**Type:** abstract (immutable, `@immutable`)
**Capabilities:** The base class for everything in the Flutter widget tree. A widget is an immutable description of part of a user interface; it is inflated into an `Element` which manages the underlying render tree. Widgets are cheap to create and are rebuilt frequently.
**Key constructors / named constructors:** `const Widget({Key? key})` — the only constructor; takes an optional `key`.
**Key properties (constructor args):**
- `key` — controls how one widget replaces another of the same type in the tree.
**Key methods:**
- `createElement()` — inflates this configuration into a concrete `Element` (implemented by subclasses).
- `canUpdate(Widget old, Widget new)` — static; true if `newWidget` can update an element holding `oldWidget` (same `runtimeType` and `key`).
- `debugFillProperties(DiagnosticPropertiesBuilder)` — adds diagnostics for the inspector/`toString`.
- `toStringShort()` / `toStringDeep()` — debug helpers.

### Element
**Type:** abstract (mention only — not a widget)
**Capabilities:** An instantiation of a `Widget` at a particular location in the tree. Elements hold the mutable structure of the tree, manage the widget lifecycle, and bridge widgets to render objects. You rarely subclass `Element` directly; every widget produces one via `createElement()`, and `BuildContext` is in fact the `Element` interface.
**Key constructors / named constructors:** `Element(Widget widget)` — abstract; created by widget subclasses.
**Key properties (constructor args):**
- `widget` — the current configuration.
- `slot`, `depth`, `renderObject`, `owner` — structural bookkeeping fields.
**Key methods:**
- `mount(parent, newSlot)`, `unmount()`, `update(newWidget)`, `rebuild()` — lifecycle.
- `markNeedsBuild()`, `performRebuild()`, `updateChild(...)` — build machinery.
- `dependOnInheritedWidgetOfExactType<T>()`, `findRenderObject()`, `visitChildren(...)` — also surfaced via `BuildContext`.

### BuildContext
**Type:** abstract interface (implemented by `Element`)
**Capabilities:** A handle to the location of a widget in the tree, passed into `build` methods. Used to look up inherited widgets, ancestor/descendant render objects and state, and the widget's size. Not a widget itself.
**Key constructors / named constructors:** None — it is an interface.
**Key properties (constructor args):**
- `widget` — the widget at this location.
- `mounted` — whether the underlying element is still in the tree.
- `size` — the render object's size (after layout).
**Key methods:**
- `dependOnInheritedWidgetOfExactType<T extends InheritedWidget>()` — establishes a dependency on an ancestor inherited widget.
- `getInheritedWidgetOfExactType<T>()` — lookup without creating a dependency.
- `findAncestorWidgetOfExactType<T>()`, `findAncestorStateOfType<T>()`, `findRenderObject()`, `findAncestorRenderObjectOfType<T>()`.
- `visitAncestorElements(...)`, `visitChildElements(...)`.

### StatelessWidget
**Type:** abstract (extends `Widget`)
**Capabilities:** Describes part of the UI that depends only on its own configuration and the `BuildContext`. Use when the widget has no mutable state and rebuilds only when its inputs or inherited dependencies change.
**Key constructors / named constructors:** `const StatelessWidget({Key? key})`.
**Key properties (constructor args):**
- `key` — inherited from `Widget`.
**Key methods:**
- `build(BuildContext context)` — the primary method; returns the subtree. Must be a pure function of configuration + context.
- `createElement()` — returns a `StatelessElement`.
- `debugFillProperties(...)`.

### StatefulWidget
**Type:** abstract (extends `Widget`)
**Capabilities:** Describes part of the UI that can change dynamically over its lifetime, with mutable state held in a companion `State` object. Use when the widget needs to maintain state that changes after build (animations, user input, async results).
**Key constructors / named constructors:** `const StatefulWidget({Key? key})`.
**Key properties (constructor args):**
- `key` — inherited from `Widget`.
**Key methods:**
- `createState()` — the primary method; creates the mutable `State`.
- `createElement()` — returns a `StatefulElement`.
- `debugFillProperties(...)`.

### State
**Type:** abstract (generic `State<T extends StatefulWidget>`; not itself a widget)
**Capabilities:** Holds the mutable state and `build` logic for a `StatefulWidget`. Persists across rebuilds of its widget. Call `setState` to signal that internal state changed and the subtree must rebuild.
**Key constructors / named constructors:** Default constructor; instances produced by `StatefulWidget.createState()`.
**Key properties (constructor args):**
- `widget` — the current configuration (updated automatically on rebuild).
- `context` — the `BuildContext` of the associated element.
- `mounted` — whether the element is in the tree.
**Key methods (lifecycle):**
- `initState()` — called once when inserted; call `super.initState()` first.
- `didChangeDependencies()` — after `initState` and whenever inherited dependencies change.
- `build(BuildContext context)` — primary method; returns the subtree.
- `didUpdateWidget(covariant T oldWidget)` — when the parent rebuilds with a new widget of the same type.
- `setState(VoidCallback fn)` — schedule a rebuild after mutating state.
- `deactivate()` — when removed from the tree (possibly temporarily).
- `activate()` — when reinserted after deactivation.
- `dispose()` — final cleanup; release controllers/listeners.
- `reassemble()` — hot reload hook.

### ProxyWidget
**Type:** abstract (extends `Widget`)
**Capabilities:** A widget with exactly one child that it does not build itself but takes as configuration. Base class for `InheritedWidget` and `ParentDataWidget`. Rarely subclassed directly.
**Key constructors / named constructors:** `const ProxyWidget({Key? key, required Widget child})`.
**Key properties (constructor args):**
- `child` — the widget below this one in the tree.
**Key methods:**
- `createElement()` — overridden by subclasses (returns a `ProxyElement`).

### InheritedWidget
**Type:** abstract (extends `ProxyWidget`)
**Capabilities:** Propagates information efficiently down the tree so descendants can read it via `BuildContext.dependOnInheritedWidgetOfExactType`. Dependents are automatically rebuilt when it changes. The backbone of most "of(context)" patterns (e.g. `Theme`, `MediaQuery`).
**Key constructors / named constructors:** `const InheritedWidget({Key? key, required Widget child})`.
**Key properties (constructor args):**
- `child` — the subtree that can access this widget.
**Key methods:**
- `updateShouldNotify(covariant InheritedWidget oldWidget)` — primary method; whether dependents must rebuild when this widget is replaced.
- `createElement()` — returns an `InheritedElement`.

### InheritedModel
**Type:** abstract (extends `InheritedWidget`)
**Capabilities:** An inherited widget that lets dependents subscribe only to a specific *aspect* of the model, so they rebuild only when that aspect changes. Use for fine-grained dependency tracking.
**Key constructors / named constructors:** `const InheritedModel({Key? key, required Widget child})`.
**Key properties (constructor args):**
- `child` — the subtree.
**Key methods:**
- `updateShouldNotify(oldWidget)` — whether to notify at all.
- `updateShouldNotifyDependent(oldWidget, Set<T> dependencies)` — whether a dependent listening to given aspects should rebuild.
- `static inheritFrom<T>(BuildContext context, {Object? aspect})` — establishes an aspect-scoped dependency.

### InheritedNotifier
**Type:** abstract (extends `InheritedWidget`, generic `<T extends Listenable>`)
**Capabilities:** Holds a `Listenable` and rebuilds its dependents whenever that listenable notifies. Bridges the `Listenable`/`ChangeNotifier` model into the inherited-widget dependency system.
**Key constructors / named constructors:** `const InheritedNotifier({Key? key, T? notifier, required Widget child})`.
**Key properties (constructor args):**
- `notifier` — the `Listenable` whose notifications trigger dependent rebuilds.
- `child` — the subtree.
**Key methods:**
- `updateShouldNotify(oldWidget)` — compares notifiers.
- `createElement()` — returns an element that listens to the notifier.

### Builder
**Type:** StatelessWidget
**Capabilities:** Delegates its `build` to a supplied callback, giving you a fresh `BuildContext` at this point in the tree. Useful to obtain a context below a newly introduced inherited widget (e.g. a context "inside" a `Scaffold` for `Scaffold.of`).
**Key constructors / named constructors:** `const Builder({Key? key, required WidgetBuilder builder})`.
**Key properties (constructor args):**
- `builder` — `Widget Function(BuildContext context)` invoked to produce the child.
**Key methods:**
- `build(context)` — returns `builder(context)`.

### StatefulBuilder
**Type:** StatefulWidget
**Capabilities:** Provides a local, self-contained piece of mutable state via a builder that receives a `StateSetter` (`setState`). Useful inside dialogs or other places where you want local state without a whole `StatefulWidget`.
**Key constructors / named constructors:** `const StatefulBuilder({Key? key, required StatefulWidgetBuilder builder})`.
**Key properties (constructor args):**
- `builder` — `Widget Function(BuildContext context, StateSetter setState)`.
**Key methods:**
- `createState()` — creates the internal state.

### ValueListenableBuilder
**Type:** StatefulWidget (generic `<T>`)
**Capabilities:** Rebuilds a portion of the tree whenever a `ValueListenable<T>` changes value, exposing the current value to a builder. An optional `child` is built once and passed through for efficiency.
**Key constructors / named constructors:** `const ValueListenableBuilder({Key? key, required ValueListenable<T> valueListenable, required ValueWidgetBuilder<T> builder, Widget? child})`.
**Key properties (constructor args):**
- `valueListenable` — the listenable being observed.
- `builder` — `Widget Function(BuildContext, T value, Widget? child)`.
- `child` — optional subtree that does not depend on the value (built once).
**Key methods:**
- `createState()` — manages the listener subscription.

### ListenableBuilder
**Type:** StatelessWidget (extends `AnimatedWidget`)
**Capabilities:** General-purpose widget that rebuilds whenever a given `Listenable` (e.g. a `ChangeNotifier`) notifies. The non-animation-specific superclass pattern for `AnimatedBuilder`. Use an optional `child` for parts that should not rebuild.
**Key constructors / named constructors:** `const ListenableBuilder({Key? key, required Listenable listenable, required TransitionBuilder builder, Widget? child})`.
**Key properties (constructor args):**
- `listenable` — the object to listen to.
- `builder` — `Widget Function(BuildContext, Widget? child)`.
- `child` — optional cached subtree.
**Key methods:**
- `build(context)` — invokes `builder(context, child)`; rebuild driven by the listenable.

### AnimatedBuilder
**Type:** StatelessWidget (extends `AnimatedWidget`)
**Capabilities:** Specialization of `ListenableBuilder` conventionally used with an `Animation`/`Listenable` to rebuild on every tick. Functionally identical to `ListenableBuilder`; prefer the latter for non-animation listenables.
**Key constructors / named constructors:** `const AnimatedBuilder({Key? key, required Listenable animation, required TransitionBuilder builder, Widget? child})`.
**Key properties (constructor args):**
- `animation` — the `Listenable`/`Animation` driving rebuilds.
- `builder`, `child` — as in `ListenableBuilder`.
**Key methods:**
- `build(context)` — invokes `builder(context, child)`.

### Notification
**Type:** abstract (not a widget)
**Capabilities:** A message that bubbles *up* the tree via `dispatch(context)`, to be caught by an ancestor `NotificationListener`. Used for child-to-ancestor communication (e.g. `ScrollNotification`).
**Key constructors / named constructors:** `Notification()`.
**Key properties (constructor args):** None intrinsic; subclasses add payload fields.
**Key methods:**
- `dispatch(BuildContext? target)` — starts the notification bubbling from `target`.
- `visitAncestor(Element element)` — internal traversal hook; returning false stops propagation.
- `debugFillProperties(...)`.

### NotificationListener
**Type:** StatelessWidget (generic `<T extends Notification>`)
**Capabilities:** Listens for notifications of type `T` bubbling up from descendants and invokes a callback. Returning `true` from the callback stops the notification propagating further up.
**Key constructors / named constructors:** `const NotificationListener({Key? key, Widget? child, NotificationListenerCallback<T>? onNotification})`.
**Key properties (constructor args):**
- `child` — the subtree to observe.
- `onNotification` — `bool Function(T notification)`; return `true` to cancel bubbling.
**Key methods:**
- `createElement()` — returns a `_NotificationElement` that intercepts notifications.

### Key
**Type:** abstract
**Capabilities:** An identifier for `Widget`s and `Element`s that controls how widgets are matched with elements when the tree rebuilds (preserving state and avoiding unnecessary rebuilds). Base class for `LocalKey` and `GlobalKey`.
**Key constructors / named constructors:** `const Key.empty()` (protected); `factory Key(String value)` → returns a `ValueKey<String>`.
**Key properties (constructor args):** None on the base type.
**Key methods:** None notable beyond `==`/`hashCode` (used during reconciliation).

### LocalKey
**Type:** abstract (extends `Key`)
**Capabilities:** A key unique only among the children of a single parent element. Base class for `ValueKey`, `ObjectKey`, and `UniqueKey`. Use to preserve/reorder state among siblings.
**Key constructors / named constructors:** `const LocalKey()`.
**Key properties (constructor args):** None.
**Key methods:** Subclasses define `==`/`hashCode`.

### ValueKey
**Type:** LocalKey (generic `<T>`)
**Capabilities:** A key that uses a value of type `T` for identity; two `ValueKey`s are equal when their values are equal. Use when a stable, comparable value (e.g. an id string) identifies a widget among siblings.
**Key constructors / named constructors:** `const ValueKey(T value)`.
**Key properties (constructor args):**
- `value` — the identity value.
**Key methods:** `==`/`hashCode` derived from `value`.

### ObjectKey
**Type:** LocalKey
**Capabilities:** A key that uses *identity* (`identical`) of a given object, rather than value equality. Use when distinct model instances should map to distinct widgets even if they compare equal.
**Key constructors / named constructors:** `const ObjectKey(Object? value)`.
**Key properties (constructor args):**
- `value` — the object whose identity is used.
**Key methods:** `==`/`hashCode` based on `identityHashCode(value)`.

### UniqueKey
**Type:** LocalKey
**Capabilities:** A key equal only to itself; every instance is distinct. Use to force a widget to be treated as new (reset state). Must be constructed once and stored if you want stability.
**Key constructors / named constructors:** `UniqueKey()` (not `const`).
**Key properties (constructor args):** None.
**Key methods:** Identity-based `==`/`hashCode`.

### PageStorageKey
**Type:** LocalKey (extends `ValueKey<T>`, generic `<T>`)
**Capabilities:** A `ValueKey` used to identify state to be saved/restored by `PageStorage` (e.g. scroll position) so it survives being scrolled out of view or routes rebuilding.
**Key constructors / named constructors:** `const PageStorageKey(T value)`.
**Key properties (constructor args):**
- `value` — identity value used for storage.
**Key methods:** Inherits `==`/`hashCode` from `ValueKey`.

### GlobalKey
**Type:** abstract (extends `Key`, generic `<T extends State<StatefulWidget>>`)
**Capabilities:** A key unique across the entire app, allowing a widget to move between parents while keeping its element/state, and providing access to its `BuildContext`, `State`, and `RenderObject` from anywhere. Use sparingly (e.g. `Form` state, imperative access).
**Key constructors / named constructors:** `factory GlobalKey({String? debugLabel})` → returns a `LabeledGlobalKey`; `const GlobalKey.constructor()` (protected).
**Key properties (constructor args):**
- `currentContext` — the `BuildContext` of the keyed element.
- `currentWidget` — the keyed widget.
- `currentState` — the associated `State` (typed `T`).
**Key methods:** Identity equality; lookups via the `current*` getters.

### GlobalObjectKey
**Type:** GlobalKey (generic `<T extends State<StatefulWidget>>`)
**Capabilities:** A `GlobalKey` whose identity derives from the identity of a given object, so the same object always yields an equal global key. Use to derive a stable global key from a model object.
**Key constructors / named constructors:** `const GlobalObjectKey(Object value)`.
**Key properties (constructor args):**
- `value` — the object whose identity backs the key.
**Key methods:** `==`/`hashCode` based on `identityHashCode(value)`; inherits the `current*` getters.

---

## B. Basic single-child layout & boxes

### Container
**Type:** StatelessWidget
**Capabilities:** A convenience widget combining common painting, positioning, and sizing widgets (padding, margins, decoration, constraints, transform). Use to quickly apply background/borders/sizing without nesting many widgets. With no child and no sizing it expands to fill; otherwise it sizes to the child.
**Key constructors / named constructors:** `Container({Key? key, AlignmentGeometry? alignment, EdgeInsetsGeometry? padding, Color? color, Decoration? decoration, Decoration? foregroundDecoration, double? width, double? height, BoxConstraints? constraints, EdgeInsetsGeometry? margin, Matrix4? transform, AlignmentGeometry? transformAlignment, Widget? child, Clip clipBehavior = Clip.none})`.
**Key properties (constructor args):**
- `alignment` — how to align the child within itself.
- `padding` / `margin` — inner/outer spacing.
- `color` — shorthand background color (mutually exclusive with `decoration`).
- `decoration` / `foregroundDecoration` — painted behind / in front of the child.
- `width` / `height` / `constraints` — sizing.
- `transform` / `transformAlignment` — paint-time matrix transform and its origin.
- `clipBehavior` — clipping when a decoration is present.
**Key methods:**
- `build(context)` — composes the appropriate primitives (`Padding`, `DecoratedBox`, `ConstrainedBox`, `Transform`, etc.).

### Padding
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Insets its child by the given amount. The most basic way to add space around a widget.
**Key constructors / named constructors:** `const Padding({Key? key, required EdgeInsetsGeometry padding, Widget? child})`.
**Key properties (constructor args):**
- `padding` — the insets to apply.
- `child` — the widget to inset.
**Key methods:**
- `createRenderObject(context)` → `RenderPadding`; `updateRenderObject(...)`, `debugFillProperties(...)`.

### Center
**Type:** SingleChildRenderObjectWidget (extends `Align`)
**Capabilities:** Centers its child within itself, optionally sizing itself to a multiple of the child's size. A specialization of `Align` with `alignment: Alignment.center`.
**Key constructors / named constructors:** `const Center({Key? key, double? widthFactor, double? heightFactor, Widget? child})`.
**Key properties (constructor args):**
- `widthFactor` / `heightFactor` — if non-null, sizes to child's width/height times the factor; otherwise expands to fill.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderPositionedBox` (inherited from `Align`).

### Align
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Aligns its child within itself and optionally sizes itself based on the child's size. Use to position a child at a corner/edge/arbitrary fractional point.
**Key constructors / named constructors:** `const Align({Key? key, AlignmentGeometry alignment = Alignment.center, double? widthFactor, double? heightFactor, Widget? child})`.
**Key properties (constructor args):**
- `alignment` — where to place the child (e.g. `Alignment.topLeft`, `Alignment(x, y)`).
- `widthFactor` / `heightFactor` — optional sizing relative to the child.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderPositionedBox`; `updateRenderObject(...)`.

### SizedBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** A box with a fixed size that forces its child (if any) to that size; a null dimension passes that axis's constraints through. Commonly used to add fixed gaps or to size a child.
**Key constructors / named constructors:**
- `const SizedBox({Key? key, double? width, double? height, Widget? child})`.
- `const SizedBox.expand({...})` — width/height = infinity (fill parent).
- `const SizedBox.shrink({...})` — width/height = 0.0 (as small as possible).
- `SizedBox.fromSize({Size? size, ...})` — size from a `Size`.
- `const SizedBox.square({double? dimension, ...})` — equal width and height.
**Key properties (constructor args):**
- `width` / `height` — the fixed dimensions (null = pass through).
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderConstrainedBox` with tight constraints; `updateRenderObject(...)`.

### ConstrainedBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Imposes additional constraints (min/max width and height) on its child, combined with the parent's constraints. Use to enforce minimum or maximum sizes.
**Key constructors / named constructors:** `ConstrainedBox({Key? key, required BoxConstraints constraints, Widget? child})`.
**Key properties (constructor args):**
- `constraints` — the `BoxConstraints` to additionally impose.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderConstrainedBox`; `updateRenderObject(...)`, `debugFillProperties(...)`.

### UnconstrainedBox
**Type:** StatelessWidget (wraps a render object)
**Capabilities:** Lets its child be laid out with unbounded (or single-axis) constraints, allowing it to take its natural size even inside a tightly constrained parent; overflow is reported in debug. Use to "escape" incoming constraints.
**Key constructors / named constructors:** `const UnconstrainedBox({Key? key, AlignmentGeometry alignment = Alignment.center, Axis? constrainedAxis, TextDirection? textDirection, Clip clipBehavior = Clip.none, Widget? child})`.
**Key properties (constructor args):**
- `alignment` — placement of the child in the freed space.
- `constrainedAxis` — if set, keeps constraints on just that axis.
- `clipBehavior` — clipping of overflow.
- `child`.
**Key methods:**
- `build(context)` — produces the underlying `RenderUnconstrainedBox` via an internal render-object widget.

### FractionallySizedBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Sizes its child to a fraction of the available space along each axis and aligns it. Use for proportional sizing (e.g. "60% of the parent's width").
**Key constructors / named constructors:** `const FractionallySizedBox({Key? key, AlignmentGeometry alignment = Alignment.center, double? widthFactor, double? heightFactor, Widget? child})`.
**Key properties (constructor args):**
- `widthFactor` / `heightFactor` — fraction of available space (null = loose pass-through on that axis).
- `alignment` — placement of the child.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderFractionallySizedOverflowBox`; `updateRenderObject(...)`.

### AspectRatio
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Sizes its child to a specific aspect ratio (width / height), as large as the constraints allow while honoring the ratio. Use for media/thumbnails that must keep proportions.
**Key constructors / named constructors:** `const AspectRatio({Key? key, required double aspectRatio, Widget? child})`.
**Key properties (constructor args):**
- `aspectRatio` — width-to-height ratio to enforce.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderAspectRatio`; `updateRenderObject(...)`, `debugFillProperties(...)`.

### FittedBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Scales and positions its child within itself according to a `BoxFit`, fitting the child into the available space. Use to scale content (often text or an icon) to fit.
**Key constructors / named constructors:** `const FittedBox({Key? key, BoxFit fit = BoxFit.contain, AlignmentGeometry alignment = Alignment.center, Clip clipBehavior = Clip.none, Widget? child})`.
**Key properties (constructor args):**
- `fit` — `contain`, `cover`, `fill`, `fitWidth`, `fitHeight`, `scaleDown`, `none`.
- `alignment` — placement within the box.
- `clipBehavior`.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderFittedBox`; `updateRenderObject(...)`.

### Baseline
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Positions its child according to the child's text baseline, shifting it so the baseline sits at a given distance from the top. Use to align widgets along a shared baseline.
**Key constructors / named constructors:** `const Baseline({Key? key, required double baseline, required TextBaseline baselineType, Widget? child})`.
**Key properties (constructor args):**
- `baseline` — distance from the top to place the child's baseline.
- `baselineType` — `alphabetic` or `ideographic`.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderBaseline`; `updateRenderObject(...)`.

### IntrinsicWidth
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Sizes its child to the child's intrinsic width. Relatively expensive (speculative layout); use only when necessary, e.g. to make a column of buttons equal width to the widest.
**Key constructors / named constructors:** `const IntrinsicWidth({Key? key, double? stepWidth, double? stepHeight, Widget? child})`.
**Key properties (constructor args):**
- `stepWidth` / `stepHeight` — round the width/height up to the nearest multiple.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderIntrinsicWidth`; `updateRenderObject(...)`.

### IntrinsicHeight
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Sizes its child to the child's intrinsic height. Commonly used to make `Row` children stretch to a common height. Expensive (speculative layout) — use sparingly.
**Key constructors / named constructors:** `const IntrinsicHeight({Key? key, Widget? child})`.
**Key properties (constructor args):**
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderIntrinsicHeight`; `updateRenderObject(...)`.

### LimitedBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Limits its child's maximum size *only when* the incoming constraint on that axis is unbounded; otherwise it has no effect. Use to give a sensible max size to children inside unbounded contexts (e.g. a scroll view).
**Key constructors / named constructors:** `const LimitedBox({Key? key, double maxWidth = double.infinity, double maxHeight = double.infinity, Widget? child})`.
**Key properties (constructor args):**
- `maxWidth` / `maxHeight` — limits applied only when the corresponding incoming constraint is unbounded.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderLimitedBox`; `updateRenderObject(...)`.

### OverflowBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Imposes different constraints on its child than it gets from its parent, allowing the child to overflow the box. Use to let a child be larger (or smaller) than the parent permits.
**Key constructors / named constructors:** `const OverflowBox({Key? key, AlignmentGeometry alignment = Alignment.center, double? minWidth, double? maxWidth, double? minHeight, double? maxHeight, Widget? child})`.
**Key properties (constructor args):**
- `minWidth` / `maxWidth` / `minHeight` / `maxHeight` — constraints imposed on the child (null = inherit).
- `alignment` — how the child is positioned within the box.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderConstrainedOverflowBox`; `updateRenderObject(...)`.

### SizedOverflowBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Has a fixed size of its own but lets its child overflow, laying the child out with the box's incoming constraints while reporting the specified `size`. Use to fix the layout size while allowing visual overflow.
**Key constructors / named constructors:** `const SizedOverflowBox({Key? key, required Size size, AlignmentGeometry alignment = Alignment.center, Widget? child})`.
**Key properties (constructor args):**
- `size` — the size this box reports to its parent.
- `alignment` — child positioning.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderSizedOverflowBox`; `updateRenderObject(...)`.

### Offstage
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Lays out its child as usual but does not paint it, hit-test it, or (when offstage) take up space. Use to keep a widget alive/measured without showing it. Still participates in the element tree (unlike removing it).
**Key constructors / named constructors:** `const Offstage({Key? key, bool offstage = true, Widget? child})`.
**Key properties (constructor args):**
- `offstage` — when true, the child is hidden and takes no space.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderOffstage`; `updateRenderObject(...)`, `debugFillProperties(...)`.

### Transform
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Applies a 4x4 `Matrix4` transformation to its child *before painting*, without affecting layout. Use for rotation, scaling, translation, skew, and perspective visual effects.
**Key constructors / named constructors:**
- `Transform({Key? key, required Matrix4 transform, Offset? origin, AlignmentGeometry? alignment, bool transformHitTests = true, FilterQuality? filterQuality, Widget? child})`.
- `Transform.rotate({required double angle, ...})` — rotation about Z.
- `Transform.translate({required Offset offset, ...})` — translation (cheaper).
- `Transform.scale({double? scale, double? scaleX, double? scaleY, ...})` — scaling.
- `Transform.flip({bool flipX = false, bool flipY = false, ...})` — mirror flips.
**Key properties (constructor args):**
- `transform` — the matrix to apply.
- `origin` / `alignment` — the point about which the transform is applied.
- `transformHitTests` — whether hit testing also uses the transform.
- `filterQuality` — sampling quality when transforming.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderTransform`; `updateRenderObject(...)`.

### FractionalTranslation
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Translates its child by a fraction of the child's own size before painting (so the offset scales with the child). Use for relative shifts (e.g. slide a widget by half its width).
**Key constructors / named constructors:** `const FractionalTranslation({Key? key, required Offset translation, bool transformHitTests = true, Widget? child})`.
**Key properties (constructor args):**
- `translation` — fractional offset (e.g. `Offset(0.5, 0)` = half the child's width).
- `transformHitTests` — whether hit testing is also translated.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderFractionalTranslation`; `updateRenderObject(...)`.

### RotatedBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Rotates its child by an integer number of quarter turns; unlike `Transform.rotate`, the rotation *affects layout* (width/height swap for odd quarter turns). Use for sideways text/labels that must occupy rotated space.
**Key constructors / named constructors:** `const RotatedBox({Key? key, required int quarterTurns, Widget? child})`.
**Key properties (constructor args):**
- `quarterTurns` — number of clockwise 90° turns.
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderRotatedBox`; `updateRenderObject(...)`.

### DecoratedBox
**Type:** SingleChildRenderObjectWidget
**Capabilities:** Paints a `Decoration` (e.g. `BoxDecoration` with color, border, gradient, shadow, image) behind or in front of its child. The primitive that `Container` uses for decoration.
**Key constructors / named constructors:** `const DecoratedBox({Key? key, required Decoration decoration, DecorationPosition position = DecorationPosition.background, Widget? child})`.
**Key properties (constructor args):**
- `decoration` — what to paint.
- `position` — `background` (behind child) or `foreground` (in front).
- `child`.
**Key methods:**
- `createRenderObject(context)` → `RenderDecoratedBox`; `updateRenderObject(...)`, `debugFillProperties(...)`.

### Placeholder
**Type:** StatelessWidget
**Capabilities:** Draws a box with a crossed-out rectangle as a visual stand-in for a widget you have not built yet. Use during prototyping/layout scaffolding.
**Key constructors / named constructors:** `const Placeholder({Key? key, Color color = const Color(0xFF455A64), double strokeWidth = 2.0, double fallbackWidth = 400.0, double fallbackHeight = 400.0, Widget? child})`.
**Key properties (constructor args):**
- `color` — line color.
- `strokeWidth` — line thickness.
- `fallbackWidth` / `fallbackHeight` — sizes used when constraints are unbounded.
- `child`.
**Key methods:**
- `build(context)` — paints via an internal `CustomPaint`.

### Spacer
**Type:** StatelessWidget
**Capabilities:** Creates flexible, empty space inside a `Flex` container (`Row`/`Column`), consuming free space proportional to its `flex`. Use to push siblings apart without a fixed-size `SizedBox`.
**Key constructors / named constructors:** `const Spacer({Key? key, int flex = 1})`.
**Key properties (constructor args):**
- `flex` — flex factor governing how much free space this spacer takes relative to other flexible children.
**Key methods:**
- `build(context)` — returns `Expanded(flex: flex, child: SizedBox.shrink())`.

---

## Multi-child layout & flex

### Row
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`, subclass of `Flex`).
**Capabilities:** Lays out a list of children horizontally along the main axis. Does not scroll; children that overflow generate a visible overflow warning in debug.
**Key constructors / named constructors:** `Row({ ... })` — no named constructors. `Row` is sugar for `Flex(direction: Axis.horizontal)`.
**Key properties (constructor args):**
- `children` — widgets laid out left-to-right (or right-to-left per text direction).
- `mainAxisAlignment` — `MainAxisAlignment` (start, end, center, spaceBetween, spaceAround, spaceEvenly).
- `mainAxisSize` — `MainAxisSize.max` (fill width) or `.min` (shrink to children).
- `crossAxisAlignment` — `CrossAxisAlignment` (start, end, center, stretch, baseline).
- `textDirection` — overrides ambient `Directionality`.
- `verticalDirection` — `VerticalDirection` ordering for the cross axis.
- `textBaseline` — required when `crossAxisAlignment == baseline`.
- `spacing` — fixed gap inserted between children.
**Key methods:** `createRenderObject` / `updateRenderObject` (returns/configures a `RenderFlex`).

### Column
**Type:** RenderObjectWidget (subclass of `Flex`).
**Capabilities:** Lays out children vertically along the main axis. Does not scroll; overflow is flagged in debug.
**Key constructors / named constructors:** `Column({ ... })` — sugar for `Flex(direction: Axis.vertical)`.
**Key properties (constructor args):**
- `children` — widgets laid out top-to-bottom.
- `mainAxisAlignment`, `mainAxisSize`, `crossAxisAlignment` — vertical/horizontal placement and sizing (`stretch` forces children to fill width).
- `textDirection`, `verticalDirection`, `textBaseline`, `spacing`.
**Key methods:** `createRenderObject` / `updateRenderObject` (`RenderFlex`).

### Flex
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`).
**Capabilities:** The base class for `Row` and `Column`; lays children in a single direction with optional flex factors. Use directly when the axis is determined at runtime.
**Key constructors / named constructors:** `Flex({ required Axis direction, ... })`.
**Key properties (constructor args):**
- `direction` — `Axis.horizontal` or `Axis.vertical`.
- `mainAxisAlignment`, `mainAxisSize`, `crossAxisAlignment`, `textDirection`, `verticalDirection`, `textBaseline`, `clipBehavior`, `spacing`.
**Key methods:** `createRenderObject` → `RenderFlex`; `updateRenderObject`; `getEffectiveTextDirection` (helper).

### Expanded
**Type:** StatelessWidget (a specialized `Flexible`).
**Capabilities:** Forces a child of a `Row`/`Column`/`Flex` to fill the available main-axis space. Equivalent to `Flexible(fit: FlexFit.tight)`.
**Key constructors / named constructors:** `Expanded({ int flex = 1, required Widget child })`.
**Key properties (constructor args):**
- `flex` — proportion of free space relative to sibling flex factors.
- `child` — must be a direct child of a flex container.
**Key methods:** Inherits from `Flexible`; works via `FlexParentData` (applies `flex`/`fit`).

### Flexible
**Type:** ParentDataWidget (`ParentDataWidget<FlexParentData>`).
**Capabilities:** Controls how a child of a flex container flexes to fill or share main-axis space, without forcing it to fill (unlike `Expanded`).
**Key constructors / named constructors:** `Flexible({ int flex = 1, FlexFit fit = FlexFit.loose, required Widget child })`.
**Key properties (constructor args):**
- `flex` — relative flex factor.
- `fit` — `FlexFit.tight` (must fill its share) or `FlexFit.loose` (may be smaller).
- `child`.
**Key methods:** `applyParentData(RenderObject)` — writes `flex`/`fit` into `FlexParentData`; `debugTypicalAncestorWidgetClass` returns `Flex`.

### Spacer
**Type:** StatelessWidget.
**Capabilities:** Flexible empty space between flex children; an idiomatic way to push siblings apart.
**Key constructors / named constructors:** `Spacer({ int flex = 1 })`.
**Key properties (constructor args):**
- `flex` — how much remaining space this gap consumes relative to other flex children.
**Key methods:** `build` — returns `Expanded(flex: flex, child: SizedBox.shrink())`.

### Stack
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`).
**Capabilities:** Overlays children on top of one another, sizing itself to its non-positioned children and positioning `Positioned` children relative to its edges. Painting order follows child order (last on top).
**Key constructors / named constructors:** `Stack({ ... })`.
**Key properties (constructor args):**
- `alignment` — `AlignmentGeometry` aligning non-positioned children (default `AlignmentDirectional.topStart`).
- `textDirection` — resolves directional alignment.
- `fit` — `StackFit.loose` / `.expand` / `.passthrough`.
- `clipBehavior` — `Clip` for children extending past the Stack (default `Clip.hardEdge`).
- `children`.
**Key methods:** `createRenderObject` → `RenderStack`; `updateRenderObject`.

### IndexedStack
**Type:** RenderObjectWidget (subclass of `Stack`).
**Capabilities:** A `Stack` that paints only a single child at the given index while keeping all children in the tree and laid out.
**Key constructors / named constructors:** `IndexedStack({ int? index = 0, ... })`.
**Key properties (constructor args):**
- `index` — the child index to display; `null` shows none.
- `alignment`, `textDirection`, `sizing`/`fit`, `clipBehavior`.
- `children` — non-visible ones still built/laid out (use `Visibility`/`Offstage` to defer cost).
**Key methods:** `createRenderObject` → `RenderIndexedStack`; `updateRenderObject`.

### Positioned
**Type:** ParentDataWidget (`ParentDataWidget<StackParentData>`).
**Capabilities:** Positions a child within a `Stack` by constraining some combination of `left`/`top`/`right`/`bottom` and optionally `width`/`height`.
**Key constructors / named constructors:**
- `Positioned({ left, top, right, bottom, width, height, required child })`.
- `Positioned.fromRect(Rect rect, ...)`, `Positioned.fromRelativeRect(RelativeRect rect, ...)`.
- `Positioned.fill({ left = 0, top = 0, right = 0, bottom = 0, ... })`.
- `Positioned.directional({ required TextDirection textDirection, start, end, ... })`.
**Key properties (constructor args):**
- `left`, `top`, `right`, `bottom` — distances from Stack edges; opposing pairs stretch the child.
- `width`, `height` — explicit dimensions.
- `child`.
**Key methods:** `applyParentData` — writes geometry into `StackParentData`; `debugTypicalAncestorWidgetClass` returns `Stack`.

### Positioned.fill
**Type:** Named constructor of `Positioned`.
**Capabilities:** Convenience constructor defaulting all four edges to `0` so the child stretches to fill the Stack.
**Key constructors / named constructors:** `Positioned.fill({ double? left = 0.0, top = 0.0, right = 0.0, bottom = 0.0, required Widget child })`.
**Key properties (constructor args):**
- `left`/`top`/`right`/`bottom` — default `0.0`; pass `null` to leave an edge unconstrained.
- `child`.
**Key methods:** Same parent-data application as `Positioned`.

### PositionedDirectional
**Type:** StatelessWidget (wraps `Positioned.directional`).
**Capabilities:** Like `Positioned` but uses `start`/`end` (resolved against ambient `Directionality`) for RTL-aware layouts.
**Key constructors / named constructors:** `PositionedDirectional({ start, top, end, bottom, width, height, required child })`.
**Key properties (constructor args):**
- `start` / `end` — distances from leading/trailing edge.
- `top`, `bottom`, `width`, `height`, `child`.
**Key methods:** `build` — reads `Directionality.of(context)` and returns a `Positioned.directional(...)`.

### Wrap
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`).
**Capabilities:** Lays children in runs along the main axis, wrapping to a new run when space runs out — like flex with line breaking.
**Key constructors / named constructors:** `Wrap({ ... })`.
**Key properties (constructor args):**
- `direction` — main run direction (default `horizontal`).
- `alignment` — `WrapAlignment` of children within a run.
- `spacing` — gap between children within a run.
- `runAlignment` — `WrapAlignment` of the runs (cross axis).
- `runSpacing` — gap between runs.
- `crossAxisAlignment` — `WrapCrossAlignment`.
- `textDirection`, `verticalDirection`, `clipBehavior`, `children`.
**Key methods:** `createRenderObject` → `RenderWrap`; `updateRenderObject`.

### Flow
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`).
**Capabilities:** Efficiently positions/transforms children via a `FlowDelegate`, applying paint-time transforms without participating in normal layout — good for animated, overlapping layouts where repositioning should not trigger relayout.
**Key constructors / named constructors:** `Flow({ required FlowDelegate delegate, List<Widget> children, Clip clipBehavior = Clip.hardEdge })`; `Flow.unwrapped(...)`.
**Key properties (constructor args):**
- `delegate` — the `FlowDelegate` controlling child sizes and per-child paint transforms.
- `children`, `clipBehavior`.
**Key methods:** `createRenderObject` → `RenderFlow`; `updateRenderObject`.

### Table
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`).
**Capabilities:** Lays children out in a grid of rows and columns with configurable column widths; all cells in a row share a height.
**Key constructors / named constructors:** `Table({ List<TableRow> children, Map<int, TableColumnWidth>? columnWidths, ... })`.
**Key properties (constructor args):**
- `children` — list of `TableRow`s.
- `columnWidths` — per-column `TableColumnWidth` map (`FixedColumnWidth`, `FlexColumnWidth`, `IntrinsicColumnWidth`, `FractionColumnWidth`).
- `defaultColumnWidth` — default `FlexColumnWidth(1.0)`.
- `textDirection`, `border` (`TableBorder`), `defaultVerticalAlignment`, `textBaseline`.
**Key methods:** `createRenderObject` → `RenderTable`; `updateRenderObject`.

### TableRow
**Type:** Plain configuration class (immutable data holder) used as `Table.children`.
**Capabilities:** Represents one row of a `Table`, holding its cell widgets and optional row-level decoration.
**Key constructors / named constructors:** `TableRow({ LocalKey? key, Decoration? decoration, List<Widget> children })`.
**Key properties (constructor args):**
- `key` — identifies the row across rebuilds.
- `decoration` — painted behind the entire row.
- `children` — one cell widget per column.
**Key methods:** None of note; consumed by `RenderTable`.

### TableCell
**Type:** ParentDataWidget (`ParentDataWidget<TableCellParentData>`).
**Capabilities:** Wraps a cell child to control its vertical alignment within its `Table` row.
**Key constructors / named constructors:** `TableCell({ TableCellVerticalAlignment? verticalAlignment, required Widget child })`.
**Key properties (constructor args):**
- `verticalAlignment` — `top`, `middle`, `bottom`, `baseline`, `fill`, `intrinsicHeight`.
- `child`.
**Key methods:** `applyParentData` — writes `verticalAlignment` into `TableCellParentData`; `debugTypicalAncestorWidgetClass` returns `Table`.

### ListBody
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`).
**Capabilities:** Arranges children sequentially along a given axis, each sized to the full cross-axis extent and natural main-axis extent — a non-scrolling, intrinsic-height list (commonly used inside a scroll view).
**Key constructors / named constructors:** `ListBody({ Axis mainAxis = Axis.vertical, bool reverse = false, List<Widget> children })`.
**Key properties (constructor args):**
- `mainAxis` — direction children are stacked.
- `reverse` — lay out in reverse order.
- `children`.
**Key methods:** `createRenderObject` → `RenderListBody`; `updateRenderObject`.

### CustomMultiChildLayout
**Type:** RenderObjectWidget (`MultiChildRenderObjectWidget`).
**Capabilities:** Delegates layout of multiple children to a `MultiChildLayoutDelegate`, letting you size/position each child explicitly (children wrapped in `LayoutId`).
**Key constructors / named constructors:** `CustomMultiChildLayout({ required MultiChildLayoutDelegate delegate, List<Widget> children })`.
**Key properties (constructor args):**
- `delegate` — implements `performLayout`/`getSize`.
- `children` — each typically wrapped in `LayoutId(id: ...)`.
**Key methods:** `createRenderObject` → `RenderCustomMultiChildLayoutBox`; `updateRenderObject`.

### CustomSingleChildLayout
**Type:** RenderObjectWidget (`SingleChildRenderObjectWidget`).
**Capabilities:** Delegates the layout (constraints, size, position) of a single child to a `SingleChildLayoutDelegate`.
**Key constructors / named constructors:** `CustomSingleChildLayout({ required SingleChildLayoutDelegate delegate, Widget? child })`.
**Key properties (constructor args):**
- `delegate`, `child`.
**Key methods:** `createRenderObject` → `RenderCustomSingleChildLayoutBox`; `updateRenderObject`.

### LayoutBuilder
**Type:** RenderObjectWidget (extends `ConstrainedLayoutBuilder<BoxConstraints>`).
**Capabilities:** Builds its subtree based on the incoming `BoxConstraints` from its parent, enabling responsive layouts. The builder runs during layout.
**Key constructors / named constructors:** `LayoutBuilder({ required Widget Function(BuildContext, BoxConstraints) builder })`.
**Key properties (constructor args):**
- `builder` — `LayoutWidgetBuilder` invoked with the parent constraints.
**Key methods:** `createRenderObject` → `RenderConstrainedLayoutBuilder`; the builder runs in `performLayout`.

### OrientationBuilder
**Type:** StatelessWidget.
**Capabilities:** Builds its subtree based on the parent's `Orientation` (portrait vs landscape), derived from whether max width exceeds max height.
**Key constructors / named constructors:** `OrientationBuilder({ required Widget Function(BuildContext, Orientation) builder })`.
**Key properties (constructor args):**
- `builder` — `OrientationWidgetBuilder` called with `Orientation.portrait` or `.landscape`.
**Key methods:** `build` — wraps a `LayoutBuilder`, computing orientation from the received constraints.

### ConstrainedLayoutBuilder
**Type:** Abstract RenderObjectWidget (generic over a `Constraints` subtype).
**Capabilities:** Shared base behind `LayoutBuilder` (and `SliverLayoutBuilder`); invokes a builder with the constraints supplied during layout and rebuilds when those constraints change. Not used directly.
**Key constructors / named constructors:** Abstract; subclasses provide a `builder`.
**Key properties (constructor args):**
- `builder` — the constraint-driven build callback.
**Key methods:** Manages layout-time build via its element and `rebuildWithConstraints`; subclasses implement `createRenderObject`/`updateRenderObject`.

### MediaQuery
**Type:** InheritedWidget (`InheritedModel<_MediaQueryAspect>`).
**Capabilities:** Propagates `MediaQueryData` (screen/window size, devicePixelRatio, padding, view insets, text scaling, orientation, platform brightness, accessibility flags) down the tree; a primary source of layout-informing metrics.
**Key constructors / named constructors:** `MediaQuery({ required MediaQueryData data, required Widget child })`; `MediaQuery.removePadding/removeViewInsets/removeViewPadding(...)`; `MediaQuery.fromView(...)`.
**Key properties (constructor args):**
- `data` — the `MediaQueryData` to expose.
- `child`.
**Key methods (static):** `MediaQuery.of(context)`; aspect-scoped `sizeOf`, `paddingOf`, `textScalerOf`, `orientationOf`, `platformBrightnessOf` (rebuild only when that aspect changes); `maybeOf`/`maybeSizeOf`.

### SafeArea
**Type:** StatelessWidget.
**Capabilities:** Insets its child with padding sufficient to avoid OS intrusions (notches, status bars, rounded corners, system gesture areas), based on `MediaQuery` padding.
**Key constructors / named constructors:** `SafeArea({ bool left = true, top = true, right = true, bottom = true, EdgeInsets minimum = EdgeInsets.zero, bool maintainBottomViewPadding = false, required Widget child })`.
**Key properties (constructor args):**
- `left`/`top`/`right`/`bottom` — whether to avoid intrusions on each side.
- `minimum` — minimum padding regardless of intrusions.
- `maintainBottomViewPadding`, `child`.
**Key methods:** `build` — reads `MediaQuery.paddingOf` and wraps the child in `Padding`. Sliver counterpart: `SliverSafeArea`.

### MultiChildLayoutDelegate
**Type:** Abstract delegate class (used by `CustomMultiChildLayout`).
**Capabilities:** Controls layout of multiple identified children; you query/position each child by its `id` (assigned via `LayoutId`).
**Key constructors / named constructors:** `MultiChildLayoutDelegate({ Listenable? relayout })`.
**Key properties:** `relayout` — optional `Listenable` driving re-layout.
**Key methods (overridable / callable):**
- `performLayout(Size size)` — **must override**; lay out and position children.
- `getSize(BoxConstraints constraints)` — overall size (default: biggest).
- `shouldRelayout(covariant MultiChildLayoutDelegate oldDelegate)` — **must override**.
- `layoutChild(Object id, BoxConstraints)` → `Size`; `positionChild(Object id, Offset)`; `hasChild(Object id)` → `bool`.

### SingleChildLayoutDelegate
**Type:** Abstract delegate class (used by `CustomSingleChildLayout`).
**Capabilities:** Controls the constraints, size, and position of a single child.
**Key constructors / named constructors:** `SingleChildLayoutDelegate({ Listenable? relayout })`.
**Key properties:** `relayout`.
**Key methods (overridable):**
- `getSize(BoxConstraints)` → `Size` (default: biggest).
- `getConstraintsForChild(BoxConstraints)` → `BoxConstraints` (default: unchanged).
- `getPositionForChild(Size size, Size childSize)` → `Offset` (default: `Offset.zero`).
- `shouldRelayout(covariant SingleChildLayoutDelegate oldDelegate)` → `bool` — **must override**.

### FlowDelegate
**Type:** Abstract delegate class (used by `Flow`).
**Capabilities:** Controls the sizes and paint-time transforms of a `Flow`'s children; positioning happens at paint via matrices, so changes are cheap (repaint, not relayout).
**Key constructors / named constructors:** `FlowDelegate({ Listenable? repaint })`.
**Key properties:** `repaint`.
**Key methods (overridable / callable):**
- `paintChildren(FlowPaintingContext context)` — **must override**; call `context.paintChild(i, transform:, opacity:)`.
- `getSize(BoxConstraints)` → `Size`; `getConstraintsForChild(int i, BoxConstraints)` → `BoxConstraints`.
- `shouldRepaint(covariant FlowDelegate oldDelegate)` → `bool` — **must override**; `shouldRelayout(...)` (default `false`).

---

## Material — app structure, scaffolding, navigation & tiles

### MaterialApp
**Type:** StatefulWidget (root convenience wrapper).
**Capabilities:** Wraps the Material-design widgets an app commonly requires, providing `WidgetsApp`, theming, localization, and a `Navigator`-based routing stack. Establishes directionality, default text styles, and the overlay used by Material widgets.
**Key constructors / named constructors:**
- `MaterialApp(...)` — imperative Navigator 1.0 API with `home`, `routes`, `onGenerateRoute`.
- `MaterialApp.router(...)` — declarative Router (Navigation 2.0) via `routerConfig` (or `routerDelegate` + `routeInformationParser` + `routeInformationProvider`); no `home`/`routes`/`navigatorKey`.
**Key properties (constructor args):**
- `home` — widget for the default `/` route.
- `routes` — named-route table (`Map<String, WidgetBuilder>`).
- `initialRoute`, `onGenerateRoute`, `onUnknownRoute`.
- `navigatorKey`, `navigatorObservers`.
- `theme`, `darkTheme`, `highContrastTheme`, `themeMode`.
- `color`, `title`, `onGenerateTitle`.
- `locale`, `supportedLocales`, `localizationsDelegates`, `localeResolutionCallback`.
- `builder` — inserts a widget above the Navigator.
- `debugShowCheckedModeBanner`, `showPerformanceOverlay`, `debugShowMaterialGrid`, `scrollBehavior`, `themeAnimationDuration`, `themeAnimationCurve`.
- `routerConfig` / `routerDelegate`, `routeInformationParser`, `routeInformationProvider`, `backButtonDispatcher` (`.router`).
**Key methods:** `createState`; internally builds `WidgetsApp`/`Navigator`. Reach what it provides via `Navigator.of(context)` and `Theme.of(context)`.

### Scaffold
**Type:** StatefulWidget.
**Capabilities:** Implements the basic Material visual layout, slotting an app bar, body, FAB, drawers, bottom bar, and snackbar/banner host into one full-screen surface. Manages drawer open/close, FAB animation, and `resizeToAvoidBottomInset`.
**Key properties (constructor args):**
- `appBar` — a `PreferredSizeWidget` (typically `AppBar`).
- `body` — the primary content.
- `floatingActionButton`, `floatingActionButtonLocation`, `floatingActionButtonAnimator`.
- `persistentFooterButtons`, `persistentFooterAlignment`.
- `drawer`, `endDrawer`, `onDrawerChanged`/`onEndDrawerChanged`.
- `bottomNavigationBar`, `bottomSheet`, `backgroundColor`.
- `resizeToAvoidBottomInset`, `primary`, `extendBody`, `extendBodyBehindAppBar`.
- `drawerDragStartBehavior`, `drawerEdgeDragWidth`, `drawerEnableOpenDragGesture`, `endDrawerEnableOpenDragGesture`, `drawerScrimColor`.
**Key methods:** `createState`; `Scaffold.of(context)` → `ScaffoldState` with `openDrawer()`, `openEndDrawer()`, `showBottomSheet()`. `Scaffold.maybeOf(context)` is the null-returning variant. (Snackbars/banners go through `ScaffoldMessenger`.)

### ScaffoldMessenger
**Type:** StatefulWidget (inherited via `ScaffoldMessengerState`).
**Capabilities:** Manages `SnackBar`s and `MaterialBanner`s for descendant `Scaffold`s, so they persist across `Scaffold` rebuilds and route transitions. Created automatically by `MaterialApp`.
**Key properties (constructor args):** `child` — the subtree it manages.
**Key methods:** `ScaffoldMessenger.of(context)` / `maybeOf(context)` → `ScaffoldMessengerState`: `showSnackBar()`, `hideCurrentSnackBar()`, `removeCurrentSnackBar()`, `clearSnackBars()`, `showMaterialBanner()`, `hideCurrentMaterialBanner()`, `removeCurrentMaterialBanner()`, `clearMaterialBanners()`. `showSnackBar` returns a `ScaffoldFeatureController`.

### AppBar
**Type:** StatefulWidget, implements `PreferredSizeWidget`.
**Capabilities:** A Material top app bar with a leading widget, title, and trailing action widgets, optionally above a bottom widget (e.g. a `TabBar`).
**Key properties (constructor args):**
- `leading`, `automaticallyImplyLeading`.
- `title`, `centerTitle`, `titleSpacing`, `titleTextStyle`.
- `actions`, `flexibleSpace`, `bottom`.
- `elevation`, `scrolledUnderElevation`, `shadowColor`, `surfaceTintColor`.
- `backgroundColor`, `foregroundColor`, `iconTheme`, `actionsIconTheme`.
- `toolbarHeight`, `leadingWidth`, `toolbarOpacity`, `bottomOpacity`, `shape`, `systemOverlayStyle`, `primary`.
**Key methods:** `createState`; `preferredSize` reports the height including any `bottom`.

### SliverAppBar
**Type:** StatefulWidget (a sliver; used inside `CustomScrollView`).
**Capabilities:** A Material app bar that integrates with a scroll view, able to float, pin, snap, and expand/collapse a `flexibleSpace`. See the scrolling/slivers section for full coverage.
**Key constructors / named constructors:** `SliverAppBar(...)`; `SliverAppBar.medium(...)`, `SliverAppBar.large(...)`.
**Key properties (constructor args):** `pinned`, `floating`, `snap`, `stretch`, `expandedHeight`, `collapsedHeight`, `flexibleSpace`, plus standard `AppBar` slots.
**Key methods:** `createState`.

### BottomAppBar
**Type:** StatelessWidget.
**Capabilities:** A Material bottom app bar (used with `Scaffold.bottomNavigationBar`) that can host a row of actions and a notch cut-out for a docked `FloatingActionButton`.
**Key properties (constructor args):**
- `child` — usually a `Row` of actions.
- `shape` — a `NotchedShape` (e.g. `CircularNotchedRectangle`).
- `notchMargin`, `color`, `surfaceTintColor`, `shadowColor`, `elevation`, `height`, `padding`, `clipBehavior`.
**Key methods:** `build`.

### FlexibleSpaceBar
**Type:** StatefulWidget.
**Capabilities:** The expanding/collapsing content placed in `AppBar.flexibleSpace` (typically within a `SliverAppBar`), animating a title and background as the bar scrolls.
**Key properties (constructor args):**
- `title`, `background`, `centerTitle`, `titlePadding`.
- `collapseMode` — `parallax`/`pin`/`none`.
- `stretchModes` — `zoomBackground`/`blurBackground`/`fadeTitle`.
- `expandedTitleScale`.
**Key methods:** `createState`; `FlexibleSpaceBar.createSettings()` injects layout settings outside the standard path.

### PreferredSize
**Type:** StatelessWidget, implements `PreferredSizeWidget`.
**Capabilities:** Wraps an arbitrary child so it can be used where a `PreferredSizeWidget` is required (e.g. `AppBar.bottom`), declaring an explicit preferred size.
**Key properties (constructor args):**
- `preferredSize` — the `Size` to reserve (often `Size.fromHeight(...)`).
- `child`.
**Key methods:** `build`; exposes `preferredSize`.

### PreferredSizeWidget
**Type:** Interface (abstract class) extending `Widget`.
**Capabilities:** Contract for widgets that report a preferred size without being laid out, enabling slots like `Scaffold.appBar` and `AppBar.bottom`. Implemented by `AppBar`, `TabBar`, `PreferredSize`.
**Key properties:** `preferredSize` — a `Size` getter. (Mention only — implement rather than instantiate.)

### BottomNavigationBar
**Type:** StatefulWidget (Material 2 style).
**Capabilities:** A bottom bar of 3–5 tappable destinations for switching between top-level views (fixed or shifting). For Material 3 prefer `NavigationBar`.
**Key properties (constructor args):**
- `items` — `BottomNavigationBarItem` list (≥ 2).
- `currentIndex`, `onTap` (`ValueChanged<int>`).
- `type` — `fixed`/`shifting`.
- `fixedColor`/`selectedItemColor`, `unselectedItemColor`, `backgroundColor`.
- `iconSize`, `selectedFontSize`, `unselectedFontSize`, `showSelectedLabels`, `showUnselectedLabels`, `selectedLabelStyle`, `unselectedLabelStyle`, `elevation`, `landscapeLayout`.
**Key methods:** `createState`.

### BottomNavigationBarItem
**Type:** Class (data descriptor, not a widget).
**Capabilities:** Describes a single destination by its icon and label.
**Key properties (constructor args):**
- `icon`, `activeIcon`, `label`, `backgroundColor`, `tooltip`.
**Key methods:** none.

### NavigationBar
**Type:** StatefulWidget (Material 3).
**Capabilities:** The M3 bottom navigation bar of 3–5 `NavigationDestination`s with an animated selection indicator pill. The modern replacement for `BottomNavigationBar`.
**Key properties (constructor args):**
- `destinations`, `selectedIndex`, `onDestinationSelected` (`ValueChanged<int>`).
- `labelBehavior` — `alwaysShow`/`onlyShowSelected`/`alwaysHide`.
- `backgroundColor`, `indicatorColor`, `indicatorShape`, `surfaceTintColor`, `shadowColor`, `elevation`, `height`, `animationDuration`, `overlayColor`.
**Key methods:** `createState`.

### NavigationDestination
**Type:** StatelessWidget (data-bearing destination).
**Capabilities:** A single destination within a Material 3 `NavigationBar`.
**Key properties (constructor args):**
- `icon`, `selectedIcon`, `label` (required), `tooltip`, `enabled`.
**Key methods:** `build`.

### NavigationRail
**Type:** StatefulWidget.
**Capabilities:** A vertical navigation surface for the side of an app (tablet/desktop), showing icon+label destinations with optional leading/trailing widgets. Pairs with a body in a `Row`.
**Key properties (constructor args):**
- `destinations` (`NavigationRailDestination`, ≥ 2), `selectedIndex`, `onDestinationSelected`.
- `extended`, `labelType` (`none`/`selected`/`all`), `leading`, `trailing`, `groupAlignment`.
- `backgroundColor`, `elevation`, `indicatorColor`, `indicatorShape`, `useIndicator`, `minWidth`, `minExtendedWidth`, `selectedIconTheme`, `unselectedIconTheme`, `selectedLabelTextStyle`, `unselectedLabelTextStyle`.
**Key methods:** `createState`.

### NavigationRailDestination
**Type:** Class (data descriptor).
**Capabilities:** Describes a single destination inside a `NavigationRail`.
**Key properties (constructor args):**
- `icon`, `selectedIcon`, `label` (required), `padding`, `disabled`.
**Key methods:** none.

### NavigationDrawer
**Type:** StatelessWidget (Material 3).
**Capabilities:** The M3 navigation drawer hosting `NavigationDrawerDestination`s (plus headers/dividers) with an animated selection indicator. Placed in `Scaffold.drawer`.
**Key properties (constructor args):**
- `children` — destinations + other widgets; destinations indexed in order, skipping non-destination children.
- `selectedIndex`, `onDestinationSelected`.
- `backgroundColor`, `surfaceTintColor`, `shadowColor`, `elevation`, `indicatorColor`, `indicatorShape`, `tilePadding`.
**Key methods:** `build`.

### NavigationDrawerDestination
**Type:** StatelessWidget (data-bearing destination).
**Capabilities:** A single selectable destination within a Material 3 `NavigationDrawer`.
**Key properties (constructor args):**
- `icon`, `selectedIcon`, `label`, `enabled`.
**Key methods:** `build`.

### Drawer
**Type:** StatelessWidget.
**Capabilities:** A Material panel that slides in horizontally from the edge of a `Scaffold` (via `drawer`/`endDrawer`). Often contains a `ListView` with a `DrawerHeader`.
**Key properties (constructor args):**
- `child`, `backgroundColor`, `elevation`, `shadowColor`, `surfaceTintColor`, `shape`, `width`, `semanticLabel`.
**Key methods:** `build`.

### DrawerHeader
**Type:** StatelessWidget.
**Capabilities:** A Material-styled header for the top of a `Drawer`, with standard padding, a bottom divider, and decoration.
**Key properties (constructor args):**
- `child`, `decoration`, `margin`, `padding`, `duration`, `curve`.
**Key methods:** `build`.

### UserAccountsDrawerHeader
**Type:** StatefulWidget.
**Capabilities:** A specialized `DrawerHeader` presenting account information — avatar(s), name, email — with an optional list of other accounts and a tap-to-toggle arrow.
**Key properties (constructor args):**
- `accountName`, `accountEmail`, `currentAccountPicture`, `currentAccountPictureSize`.
- `otherAccountsPictures`, `otherAccountsPicturesSize`, `onDetailsPressed`, `decoration`, `margin`, `arrowColor`.
**Key methods:** `createState`.

### FloatingActionButton
**Type:** StatelessWidget.
**Capabilities:** A circular (or pill-shaped extended) Material button that floats above content to promote a primary action. Positioned by `Scaffold` via `floatingActionButtonLocation`.
**Key constructors / named constructors:** `FloatingActionButton(...)`, `.small(...)`, `.large(...)`, `.extended(...)` (pill with `label` + optional `icon`; no `child`).
**Key properties (constructor args):**
- `onPressed`, `child` (usually an `Icon`; not for `.extended`).
- `label`, `icon`, `extendedIconLabelSpacing`, `extendedPadding`, `extendedTextStyle` (`.extended`).
- `tooltip`, `backgroundColor`, `foregroundColor`, `splashColor`, `focusColor`, `hoverColor`.
- `elevation`, `focusElevation`, `hoverElevation`, `highlightElevation`, `disabledElevation`.
- `shape`, `mini`, `heroTag`, `isExtended`, `clipBehavior`, `materialTapTargetSize`, `enableFeedback`, `mouseCursor`.
**Key methods:** `build`.

### FloatingActionButtonLocation
**Type:** Abstract class (predefined location constants).
**Capabilities:** Defines where the `Scaffold` places its FAB. Static instances like `endFloat`, `centerFloat`, `endDocked`, `centerDocked`, `startTop`, `endTop`, `miniEndFloat`. (Pass an instance to `Scaffold.floatingActionButtonLocation`.)
**Key methods:** `getOffset(ScaffoldPrelayoutGeometry)` (overridden by subclasses).

### TabBar
**Type:** StatefulWidget, implements `PreferredSizeWidget`.
**Capabilities:** A horizontal row of Material tabs, typically in `AppBar.bottom`, that drives or follows a `TabController`.
**Key constructors / named constructors:** `TabBar(...)`; `TabBar.secondary(...)`.
**Key properties (constructor args):**
- `tabs`, `controller` (optional if a `DefaultTabController` is above).
- `isScrollable`, `tabAlignment`, `onTap`.
- `indicator`, `indicatorColor`, `indicatorWeight`, `indicatorPadding`, `indicatorSize`.
- `labelColor`, `unselectedLabelColor`, `labelStyle`, `unselectedLabelStyle`, `labelPadding`.
- `dividerColor`, `dividerHeight`, `overlayColor`, `splashFactory`, `padding`, `physics`, `mouseCursor`.
**Key methods:** `createState`; `preferredSize`.

### TabBarView
**Type:** StatefulWidget.
**Capabilities:** The page area paired with a `TabBar`, showing the selected tab's widget and animating swipe transitions, synced to a `TabController`.
**Key properties (constructor args):**
- `children` (length must match the controller), `controller`, `physics`, `dragStartBehavior`, `viewportFraction`, `clipBehavior`.
**Key methods:** `createState`.

### TabController
**Type:** Class (a `ChangeNotifier`, not a widget).
**Capabilities:** Coordinates tab selection between a `TabBar` and `TabBarView`, tracking the current index and the animation between tabs. Requires a `TickerProvider` (`vsync`); often supplied by `DefaultTabController`.
**Key constructors / named constructors:** `TabController({ required int length, int initialIndex = 0, required TickerProvider vsync, Duration? animationDuration })`.
**Key properties / methods:**
- `index`, `previousIndex`, `indexIsChanging`, `offset`, `animation`.
- `animateTo(int index, {duration, curve})`, `addListener`/`removeListener`, `dispose`.

### DefaultTabController
**Type:** StatefulWidget (provides a `TabController` via an `InheritedWidget`).
**Capabilities:** Creates and shares a `TabController` with its subtree, so `TabBar`/`TabBarView` can find it implicitly.
**Key properties (constructor args):**
- `length` (required), `initialIndex`, `animationDuration`, `child`.
**Key methods:** `DefaultTabController.of(context)` → the ambient `TabController`; `maybeOf(context)`.

### Tab
**Type:** StatelessWidget, implements `PreferredSizeWidget`.
**Capabilities:** A single Material tab label for `TabBar.tabs`, displaying text, an icon, and/or a custom child.
**Key properties (constructor args):**
- `text` (mutually exclusive with `child`), `icon`, `child`, `iconMargin`, `height`.
**Key methods:** `build`; `preferredSize`.

### TabPageSelector
**Type:** StatelessWidget.
**Capabilities:** A row of small dots indicating the current tab/page within a `TabController` — a lightweight page indicator.
**Key properties (constructor args):**
- `controller`, `indicatorSize`, `color`, `selectedColor`, `borderStyle`, `borderColor`.
**Key methods:** `build`.

### ListTile
**Type:** StatelessWidget.
**Capabilities:** A fixed-height single row with leading widget, title/subtitle text, and trailing widget — the standard building block for lists and menus. Provides tap, selection, and density styling.
**Key properties (constructor args):**
- `leading`, `title`, `subtitle`, `trailing`.
- `isThreeLine`, `dense`, `visualDensity`.
- `onTap`, `onLongPress`, `enabled`, `selected`.
- `contentPadding`, `horizontalTitleGap`, `minVerticalPadding`, `minLeadingWidth`, `minTileHeight`.
- `tileColor`, `selectedTileColor`, `iconColor`, `textColor`, `selectedColor`, `shape`, `titleAlignment`, `mouseCursor`, `focusColor`, `hoverColor`, `splashColor`, `autofocus`, `enableFeedback`, `titleTextStyle`, `subtitleTextStyle`, `leadingAndTrailingTextStyle`.
**Key methods:** `build`; `ListTile.divideTiles({context, tiles, color})` — static helper inserting dividers.

### ListTileTheme
**Type:** StatelessWidget (`InheritedTheme`).
**Capabilities:** Provides default `ListTile` styling (colors, density, padding, shape) to descendant `ListTile`s.
**Key constructors / named constructors:** `ListTileTheme(...)`; `ListTileTheme.merge(...)`.
**Key properties (constructor args):** `data` (a `ListTileThemeData`) or individual overrides (`tileColor`, `selectedTileColor`, `iconColor`, `textColor`, `contentPadding`, `dense`, `shape`, `style`, `horizontalTitleGap`, `minVerticalPadding`, `minLeadingWidth`); `child`.
**Key methods:** `ListTileTheme.of(context)` → the effective `ListTileThemeData`.

### AboutListTile
**Type:** StatelessWidget.
**Capabilities:** A ready-made `ListTile` (e.g. for a drawer) that opens an `AboutDialog` showing the app's legalese and licenses when tapped.
**Key properties (constructor args):**
- `icon`, `child`, `applicationName`, `applicationVersion`, `applicationIcon`, `applicationLegalese`, `aboutBoxChildren`, `dense`.
**Key methods:** `build` (delegates to `showAboutDialog`).

### CheckboxListTile
**Type:** StatelessWidget.
**Capabilities:** A `ListTile` whose control is a `Checkbox`, combining a label with a boolean toggle. (Cross-ref: selection-controls section; siblings `RadioListTile`, `SwitchListTile`.)
**Key properties (constructor args):** `value`, `onChanged`, `title`, `subtitle`, `secondary`, `controlAffinity`, `tristate`, `activeColor`, `checkColor`, `selected`, `dense`, `isThreeLine`.
**Key methods:** `build`.

### GridTile
**Type:** StatelessWidget.
**Capabilities:** A tile for a grid (e.g. `GridView`) that overlays an optional header and/or footer bar (commonly `GridTileBar`) on top of a child.
**Key properties (constructor args):**
- `child` (required, typically an image), `header`, `footer`.
**Key methods:** `build`.

### GridTileBar
**Type:** StatelessWidget.
**Capabilities:** A semi-transparent Material bar designed for the `header`/`footer` of a `GridTile`, holding leading/title/subtitle/trailing widgets over the image.
**Key properties (constructor args):**
- `leading`, `title`, `subtitle`, `trailing`, `backgroundColor`.
**Key methods:** `build`.

### ExpansionTile
**Type:** StatefulWidget.
**Capabilities:** A `ListTile`-like row that expands to reveal its `children` when tapped, animating a trailing chevron. Useful for collapsible list sections.
**Key constructors / named constructors:** `ExpansionTile(...)`; can be driven via an `ExpansionTileController` (`controller`).
**Key properties (constructor args):**
- `title`, `subtitle`, `leading`, `trailing`, `children`.
- `initiallyExpanded`, `maintainState`, `onExpansionChanged`, `controlAffinity`.
- `tilePadding`, `childrenPadding`, `expandedAlignment`, `expandedCrossAxisAlignment`.
- `backgroundColor`, `collapsedBackgroundColor`, `iconColor`, `collapsedIconColor`, `textColor`, `collapsedTextColor`, `shape`, `collapsedShape`, `clipBehavior`, `controller`, `dense`, `visualDensity`, `enableFeedback`, `showTrailingIcon`.
**Key methods:** `createState`; via `ExpansionTileController.of(context)`: `expand()`, `collapse()`, `isExpanded`.

### ExpansionPanel
**Type:** Class (configuration object, not a widget).
**Capabilities:** Describes one expandable panel within an `ExpansionPanelList`, with a header builder, a body, and its expansion state.
**Key properties (constructor args):**
- `headerBuilder` (`ExpansionPanelHeaderBuilder`), `body`, `isExpanded`, `canTapOnHeader`, `backgroundColor`.
**Key methods:** none; subclass `ExpansionPanelRadio` adds a `value` for radio behavior.

### ExpansionPanelList
**Type:** StatefulWidget.
**Capabilities:** Renders a Material list of `ExpansionPanel`s that expand/collapse, reporting toggle requests via a callback. The `.radio` variant allows only one panel open at a time.
**Key constructors / named constructors:** `ExpansionPanelList(...)`; `ExpansionPanelList.radio(...)`.
**Key properties (constructor args):**
- `children`, `expansionCallback` (`ExpansionPanelCallback(int panelIndex, bool isExpanded)`), `animationDuration`.
- `expandedHeaderPadding`, `dividerColor`, `elevation`, `expandIconColor`, `materialGapSize`, `initialOpenPanelValue` (`.radio`).
**Key methods:** `createState`.

### Card
**Type:** StatelessWidget.
**Capabilities:** A Material surface with rounded corners and elevation/shadow for grouping related content. M3 offers elevated (default), filled, and outlined variants.
**Key constructors / named constructors:** `Card(...)`, `Card.filled(...)`, `Card.outlined(...)`.
**Key properties (constructor args):**
- `child`, `color`, `shadowColor`, `surfaceTintColor`, `elevation`, `shape`, `margin`, `borderOnForeground`, `clipBehavior`, `semanticContainer`.
**Key methods:** `build`.

### Material
**Type:** StatefulWidget.
**Capabilities:** A piece of Material — the surface on which ink reactions appear and which provides elevation, shape clipping, and shadow. Most Material widgets require an ancestor `Material`.
**Key properties (constructor args):**
- `child`, `type` (`canvas`/`card`/`circle`/`button`/`transparency`), `elevation`, `color`, `shadowColor`, `surfaceTintColor`, `shape`, `borderRadius`, `borderOnForeground`, `clipBehavior`, `textStyle`, `animationDuration`.
**Key methods:** `createState`; `Material.of(context)` → the nearest `MaterialInkController`.

### InkWell
**Type:** StatelessWidget (a rectangular `InkResponse`).
**Capabilities:** A rectangular area that responds to touch with Material ink splash and highlight, requiring an ancestor `Material`. The standard way to make widgets tappable with feedback.
**Key properties (constructor args):**
- `onTap`, `onDoubleTap`, `onLongPress`, `onTapDown`, `onTapUp`, `onTapCancel`, `onHover`, `onFocusChange`, `child`.
- `splashColor`, `highlightColor`, `focusColor`, `hoverColor`, `overlayColor`, `splashFactory`, `borderRadius`, `customBorder`, `radius`, `enableFeedback`, `excludeFromSemantics`, `mouseCursor`, `focusNode`, `autofocus`, `canRequestFocus`, `statesController`.
**Key methods:** `build`.

### InkResponse
**Type:** StatelessWidget.
**Capabilities:** Like `InkWell` but with configurable ink shape (circular vs. rectangle) and bounds; its more general base. Produces ink reactions on tap within an ancestor `Material`.
**Key properties (constructor args):**
- Gesture/hover callbacks as in `InkWell`.
- `containedInkWell`, `highlightShape` (`circle`/`rectangle`), `radius`, `borderRadius`, `customBorder`.
- `splashColor`, `highlightColor`, `focusColor`, `hoverColor`, `overlayColor`, `splashFactory`, `mouseCursor`, `enableFeedback`, `excludeFromSemantics`, `focusNode`, `autofocus`, `canRequestFocus`, `statesController`.
**Key methods:** `build`.

### Ink
**Type:** StatefulWidget.
**Capabilities:** Paints a decoration (color or `BoxDecoration`/image) onto the nearest `Material` so ink splashes render above the decoration. Use instead of a `Container` decoration when the area is inkable.
**Key constructors / named constructors:** `Ink(...)`; `Ink.image(...)`.
**Key properties (constructor args):**
- `decoration` (mutually exclusive with `color`), `color`, `image`/`fit`/`alignment` (`.image`), `child`, `width`, `height`, `padding`, `margin`.
**Key methods:** `createState`.

### InkDecoration
**Type:** Class (an `InkFeature`, not a widget).
**Capabilities:** The low-level ink feature that paints a `Decoration` on a `Material`; what the `Ink` widget creates internally. (Use the `Ink` widget rather than constructing this directly.)
**Key methods:** `paintFeature`, `dispose`.

### MaterialButton
**Type:** StatelessWidget (legacy).
**Capabilities:** The older, pre-`ButtonStyle` Material button (base for the deprecated `RaisedButton`/`FlatButton`/`OutlineButton`). Prefer `ElevatedButton`/`TextButton`/`OutlinedButton`/`FilledButton` in new code.
**Key properties (constructor args):**
- `onPressed`, `onLongPress`, `child`.
- `color`, `textColor`, `disabledColor`, `disabledTextColor`, `splashColor`, `highlightColor`, `focusColor`, `hoverColor`.
- `elevation`, `focusElevation`, `hoverElevation`, `highlightElevation`, `disabledElevation`.
- `padding`, `shape`, `clipBehavior`, `materialTapTargetSize`, `minWidth`, `height`, `colorBrightness`, `mouseCursor`, `visualDensity`, `enableFeedback`.
**Key methods:** `build`.

### ButtonBar
**Type:** StatelessWidget (deprecated).
**Capabilities:** Lays out a row of buttons (e.g. dialog actions) with consistent spacing, wrapping to a column when constrained. Deprecated in favor of `OverflowBar`/`Row`.
**Key properties (constructor args):** `children`, `alignment`, `mainAxisSize`, `buttonPadding`, `buttonMinWidth`, `buttonHeight`, `buttonAlignedDropdown`, `overflowDirection`, `overflowButtonSpacing`, `layoutBehavior`.
**Key methods:** `build`.

### SnackBar
**Type:** StatefulWidget.
**Capabilities:** A lightweight, briefly-shown message at the bottom of the screen (with optional action), surfaced via `ScaffoldMessenger.of(context).showSnackBar(...)`. Auto-dismisses and can be swiped away.
**Key constructors / named constructors:** `SnackBar(...)`.
**Key properties (constructor args):**
- `content` (required), `action` (a `SnackBarAction`).
- `backgroundColor`, `elevation`, `margin`, `padding`, `width`, `shape`.
- `behavior` (`fixed`/`floating`), `duration`, `dismissDirection`, `onVisible`, `showCloseIcon`, `closeIconColor`, `clipBehavior`, `hitTestBehavior`, `animation`, `actionOverflowThreshold`.
**Key methods:** `createState`. Display via `ScaffoldMessenger`.

### SnackBarAction
**Type:** StatefulWidget.
**Capabilities:** A single action button (e.g. "UNDO") displayed within a `SnackBar`.
**Key properties (constructor args):**
- `label` (required), `onPressed` (required; the snackbar dismisses after it runs), `textColor`, `disabledTextColor`, `backgroundColor`, `disabledBackgroundColor`.
**Key methods:** `createState`.

### MaterialBanner
**Type:** StatefulWidget.
**Capabilities:** A persistent, prominent message at the top of the content area with one or more action buttons; shown via `ScaffoldMessenger.of(context).showMaterialBanner(...)` and dismissed explicitly.
**Key properties (constructor args):**
- `content` (required), `actions` (required), `leading`.
- `backgroundColor`, `surfaceTintColor`, `shadowColor`, `dividerColor`, `elevation`, `contentTextStyle`, `padding`, `leadingPadding`, `margin`, `forceActionsBelow`, `overflowAlignment`, `onVisible`.
**Key methods:** `createState`. Display/clear via `ScaffoldMessenger`.

### showAboutDialog
**Type:** Top-level function.
**Capabilities:** Shows an `AboutDialog` describing the application without constructing the dialog yourself.
**Key properties (parameters):**
- `context` (required), `applicationName`, `applicationVersion`, `applicationIcon`, `applicationLegalese`, `children`, `useRootNavigator`, `routeSettings`, `anchorPoint`, `barrierDismissible`, `barrierColor`, `barrierLabel`.
**Key methods:** n/a (returns `void`/`Future`).

### AboutDialog
**Type:** StatelessWidget.
**Capabilities:** An "about" dialog box showing the app's name, version, icon, legalese, and a button linking to the bundled `LicensePage`. Usually shown via `showAboutDialog` or `AboutListTile`.
**Key properties (constructor args):**
- `applicationName`, `applicationVersion`, `applicationIcon`, `applicationLegalese`, `children`.
**Key methods:** `build`.

---

## Material — buttons, selection, dialogs & feedback

### ElevatedButton
**Type:** StatelessWidget (a `ButtonStyleButton`).
**Capabilities:** A Material "elevated" button: a filled button with a shadow that lifts off the surface, to emphasize actions on flat layouts.
**Key constructors / named constructors:** `ElevatedButton(...)` (takes a `child`); `ElevatedButton.icon({icon, label, ...})`.
**Key properties (constructor args):**
- `onPressed` (`VoidCallback?`; null disables), `onLongPress`, `onHover`, `onFocusChange`.
- `style` (`ButtonStyle?`, often via `ElevatedButton.styleFrom`), `child`, `autofocus`, `clipBehavior`, `focusNode`, `statesController`.
**Key methods:** `static ButtonStyle styleFrom({...})`; `build`; `defaultStyleOf` / `themeStyleOf`.

### FilledButton
**Type:** StatelessWidget (a `ButtonStyleButton`).
**Capabilities:** A Material 3 filled button using a solid fill for high emphasis; the `.tonal` variant uses a secondary-container fill for medium emphasis.
**Key constructors / named constructors:** `FilledButton(...)`, `.icon({icon, label})`, `.tonal(...)`, `.tonalIcon({icon, label})`.
**Key properties (constructor args):**
- `onPressed`, `onLongPress`, `onHover`, `onFocusChange`, `style`, `child`, `autofocus`, `clipBehavior`, `focusNode`, `statesController`.
**Key methods:** `static ButtonStyle styleFrom({...})`; `build`.

### FilledButton.tonal
**Type:** Named constructor of `FilledButton`.
**Capabilities:** A tonal filled button (secondary-container fill) for medium emphasis — between `FilledButton` and `OutlinedButton`.
**Key constructors / named constructors:** `FilledButton.tonal(...)`, `.tonalIcon({icon, label})`.
**Key properties (constructor args):** Same as `FilledButton`; differs only in default styling.
**Key methods:** Shares `FilledButton.styleFrom`.

### OutlinedButton
**Type:** StatelessWidget (a `ButtonStyleButton`).
**Capabilities:** A medium-emphasis button with a text label and a visible outline but no fill.
**Key constructors / named constructors:** `OutlinedButton(...)`, `.icon({icon, label})`.
**Key properties (constructor args):**
- `onPressed`, `onLongPress`, `onHover`, `onFocusChange`, `style` (border via `side`), `child`, `autofocus`, `clipBehavior`, `focusNode`, `statesController`.
**Key methods:** `static ButtonStyle styleFrom({...})`; `build`.

### TextButton
**Type:** StatelessWidget (a `ButtonStyleButton`).
**Capabilities:** A low-emphasis button with no border or fill (text only), used in dialogs, toolbars, and inline.
**Key constructors / named constructors:** `TextButton(...)`, `.icon({icon, label})`.
**Key properties (constructor args):**
- `onPressed`, `onLongPress`, `onHover`, `onFocusChange`, `style`, `child`, `autofocus`, `clipBehavior`, `focusNode`, `statesController`.
**Key methods:** `static ButtonStyle styleFrom({...})`; `build`.

### IconButton
**Type:** StatelessWidget.
**Capabilities:** A clickable Material icon with ink splash and tooltip support; in Material 3 offers filled/tonal/outlined variants.
**Key constructors / named constructors:** `IconButton(...)`, `.filled(...)`, `.filledTonal(...)`, `.outlined(...)`.
**Key properties (constructor args):**
- `icon` (the `Widget`, usually an `Icon`), `onPressed` (null disables).
- `iconSize`, `color`, `disabledColor`, `splashRadius`, `padding`, `alignment`, `constraints`, `tooltip`.
- `isSelected` + `selectedIcon` (togglable, M3), `style`, `visualDensity`, `autofocus`, `focusNode`.
**Key methods:** `static ButtonStyle styleFrom({...})`; `build`.

### SegmentedButton
**Type:** StatefulWidget (generic `SegmentedButton<T>`).
**Capabilities:** A Material 3 single- or multi-select control showing a row of connected segments (`ButtonSegment<T>`); reflects selection with checkmarks.
**Key constructors / named constructors:** `SegmentedButton({required segments, required selected, onSelectionChanged, ...})`.
**Key properties (constructor args):**
- `segments` (`List<ButtonSegment<T>>`: `value`, `label`, `icon`, `enabled`).
- `selected` (`Set<T>`), `onSelectionChanged` (`ValueChanged<Set<T>>?`; null disables).
- `multiSelectionEnabled`, `emptySelectionAllowed`, `showSelectedIcon`, `selectedIcon`, `style`, `direction`.
**Key methods:** `createState`.

### ToggleButtons
**Type:** StatefulWidget.
**Capabilities:** A horizontal/vertical set of toggle buttons sharing a border; the parent tracks selection via a parallel boolean list (older control, predates `SegmentedButton`).
**Key constructors / named constructors:** `ToggleButtons({required children, required isSelected, onPressed, ...})`.
**Key properties (constructor args):**
- `children`, `isSelected` (`List<bool>`), `onPressed` (`void Function(int)?`; null disables).
- `direction`, `verticalDirection`, `color`, `selectedColor`, `disabledColor`, `fillColor`, `borderColor`, `selectedBorderColor`, `borderRadius`, `borderWidth`, `constraints`.
**Key methods:** `createState`.

### DropdownButton
**Type:** StatefulWidget (generic `DropdownButton<T>`).
**Capabilities:** A Material dropdown that shows the selected value and, when tapped, a menu of `DropdownMenuItem<T>` options.
**Key constructors / named constructors:** `DropdownButton({required items, value, onChanged, ...})`.
**Key properties (constructor args):**
- `items` (`List<DropdownMenuItem<T>>?`), `value`, `onChanged` (null disables), `hint`, `disabledHint`.
- `icon`, `iconSize`, `iconEnabledColor`, `iconDisabledColor`, `isExpanded`, `isDense`, `underline`, `style`, `dropdownColor`, `elevation`, `menuMaxHeight`, `selectedItemBuilder`, `onTap`, `borderRadius`.
**Key methods:** `createState`.

### DropdownButtonFormField
**Type:** StatefulWidget — a `FormField<T>` wrapping a `DropdownButton`.
**Capabilities:** Integrates a dropdown into a `Form`, adding validation, `InputDecoration`, and save/validate lifecycle.
**Key constructors / named constructors:** `DropdownButtonFormField({required items, value/initialValue, onChanged, ...})`.
**Key properties (constructor args):**
- `items`, `value`/`initialValue`, `onChanged`, `hint`, `disabledHint`, `decoration`, `validator`, `onSaved`, `autovalidateMode`.
- Passthroughs: `icon`, `isExpanded`, `isDense`, `style`, `dropdownColor`, `elevation`, `menuMaxHeight`, `borderRadius`, `selectedItemBuilder`.
**Key methods:** `createState` (inherited `FormField` save/validate/reset).

### DropdownMenu
**Type:** StatefulWidget (generic `DropdownMenu<T>`), Material 3.
**Capabilities:** A combined text field + dropdown menu allowing selection from `DropdownMenuEntry<T>` items and optional free-text filtering/searching.
**Key constructors / named constructors:** `DropdownMenu({required dropdownMenuEntries, ...})`.
**Key properties (constructor args):**
- `dropdownMenuEntries` (`value`, `label`, `leadingIcon`, `enabled`), `initialSelection`, `onSelected`.
- `controller`, `enableFilter`, `enableSearch`, `requestFocusOnTap`, `label`, `hintText`, `helperText`, `leadingIcon`, `trailingIcon`, `width`, `menuHeight`, `inputDecorationTheme`, `menuStyle`, `expandedInsets`.
**Key methods:** `createState`.

### MenuAnchor
**Type:** StatefulWidget.
**Capabilities:** A Material 3 primitive that anchors a floating menu (overlay) to a child; controlled via a `MenuController` with `MenuItemButton`/`SubmenuButton` children.
**Key constructors / named constructors:** `MenuAnchor({required menuChildren, builder, child, ...})`.
**Key properties (constructor args):**
- `menuChildren`, `builder` (`(context, MenuController, child?)`), `controller`, `child`, `style`, `alignmentOffset`, `clipBehavior`, `consumeOutsideTap`, `onOpen`, `onClose`, `crossAxisUnconstrained`.
**Key methods:** `createState`; works with `MenuController.open()/close()`.

### MenuBar
**Type:** StatefulWidget.
**Capabilities:** A Material 3 horizontal application menu bar (desktop-style) composed of `SubmenuButton`s that open cascading menus.
**Key constructors / named constructors:** `MenuBar({required children, ...})`.
**Key properties (constructor args):**
- `children` (typically `SubmenuButton`s), `controller`, `style`, `clipBehavior`.
**Key methods:** `createState`.

### PopupMenuButton
**Type:** StatefulWidget (generic `PopupMenuButton<T>`).
**Capabilities:** A button (default: overflow/more icon) that opens a popup menu when pressed, invoking `onSelected` with the chosen value.
**Key constructors / named constructors:** `PopupMenuButton({required itemBuilder, ...})`.
**Key properties (constructor args):**
- `itemBuilder` (`PopupMenuItemBuilder<T>`), `onSelected`, `onCanceled`, `initialValue`, `icon`, `child`.
- `tooltip`, `elevation`, `color`, `shape`, `padding`, `offset`, `position`, `enabled`, `constraints`, `splashRadius`, `iconSize`, `menuPadding`.
**Key methods:** `createState`.

### PopupMenuItem
**Type:** StatefulWidget (generic `PopupMenuItem<T>`), a `PopupMenuEntry<T>`.
**Capabilities:** A single selectable row within a popup menu; carries a `value`. Related: `PopupMenuDivider`, `CheckedPopupMenuItem`.
**Key constructors / named constructors:** `PopupMenuItem({required child, value, ...})`.
**Key properties (constructor args):**
- `value`, `child`, `enabled`, `onTap`, `height`, `padding`, `textStyle`, `labelTextStyle`, `mouseCursor`.
**Key methods:** `createState`; `height`/`represents` (from `PopupMenuEntry`).

### BackButton
**Type:** StatelessWidget.
**Capabilities:** A platform-aware "back" `IconButton` that by default pops the current route via `Navigator.maybePop`.
**Key constructors / named constructors:** `BackButton({onPressed, color, style, ...})`.
**Key properties (constructor args):** `onPressed`, `color`, `style`.
**Key methods:** `build`.

### CloseButton
**Type:** StatelessWidget.
**Capabilities:** An `IconButton` showing a close (X) icon that by default pops the current route; used to dismiss pages, dialogs, or modal surfaces.
**Key constructors / named constructors:** `CloseButton({onPressed, color, style, ...})`.
**Key properties (constructor args):** `onPressed`, `color`, `style`.
**Key methods:** `build`.

### FloatingActionButton
**Type:** StatelessWidget. *(Full detail in the app-structure section.)*
**Capabilities:** A circular (or extended) prominent action button that floats above content, typically anchored by `Scaffold.floatingActionButton`.
**Key constructors / named constructors:** `FloatingActionButton(...)`, `.small`, `.large`, `.extended({icon, label})`.
**Key properties (constructor args):** `onPressed`, `child`, `tooltip`, `backgroundColor`, `foregroundColor`, `elevation`, `heroTag`, `mini`, `shape`.
**Key methods:** `build`.

### ButtonStyle
**Type:** Class (`@immutable`, supporting type, not a widget).
**Capabilities:** Holds the resolvable visual properties of the `ButtonStyleButton` family; nearly all properties are `MaterialStateProperty<T>` so they vary by interaction state.
**Key constructors / named constructors:** `ButtonStyle({backgroundColor, foregroundColor, overlayColor, elevation, padding, side, shape, ...})`.
**Key properties (constructor args):**
- `backgroundColor`, `foregroundColor`, `overlayColor`, `shadowColor`, `surfaceTintColor` (`MaterialStateProperty<Color?>`).
- `elevation`, `padding`, `minimumSize`, `maximumSize`, `fixedSize`, `side`, `shape`.
- `mouseCursor`, `visualDensity`, `tapTargetSize`, `animationDuration`, `enableFeedback`, `alignment`, `splashFactory`, `textStyle`, `iconColor`, `iconSize`.
**Key methods:** `copyWith`, `merge`, `static lerp`. Each button family also exposes `styleFrom({...})`.

### Checkbox
**Type:** StatefulWidget.
**Capabilities:** A Material checkbox (checked/unchecked, optionally tristate/null); controlled — the parent updates `value` in response to `onChanged`.
**Key constructors / named constructors:** `Checkbox({required value, required onChanged, ...})`.
**Key properties (constructor args):**
- `value` (`bool?`), `onChanged` (null disables), `tristate`.
- `activeColor`, `checkColor`, `fillColor`, `overlayColor`, `side`, `shape`, `materialTapTargetSize`, `visualDensity`, `focusNode`, `autofocus`, `isError`, `semanticLabel`.
**Key methods:** `createState`.

### CheckboxListTile
**Type:** StatelessWidget.
**Capabilities:** A `ListTile` combined with a `Checkbox`, giving a tappable row that toggles the checkbox.
**Key constructors / named constructors:** `CheckboxListTile({required value, required onChanged, ...})`, `.adaptive(...)`.
**Key properties (constructor args):**
- `value`, `onChanged`, `tristate`, `title`, `subtitle`, `secondary`, `controlAffinity`, `activeColor`, `checkColor`, `tileColor`, `selected`, `dense`, `contentPadding`, `enabled`.
**Key methods:** `build`.

### Radio
**Type:** StatefulWidget (generic `Radio<T>`).
**Capabilities:** A single radio button representing one option in a mutually exclusive group; selected when `value == groupValue`.
**Key constructors / named constructors:** `Radio({required value, required groupValue, required onChanged, ...})`.
**Key properties (constructor args):**
- `value`, `groupValue`, `onChanged` (null disables), `toggleable`.
- `activeColor`, `fillColor`, `overlayColor`, `materialTapTargetSize`, `visualDensity`, `focusNode`, `autofocus`.
**Key methods:** `createState`.

### RadioListTile
**Type:** StatelessWidget (generic `RadioListTile<T>`).
**Capabilities:** A `ListTile` paired with a `Radio<T>`, producing a tappable row that selects its `value` into the group.
**Key constructors / named constructors:** `RadioListTile({required value, required groupValue, required onChanged, ...})`, `.adaptive(...)`.
**Key properties (constructor args):**
- `value`, `groupValue`, `onChanged`, `toggleable`, `title`, `subtitle`, `secondary`, `controlAffinity`, `activeColor`, `tileColor`, `selected`, `dense`, `contentPadding`, `enabled`.
**Key methods:** `build`.

### Switch
**Type:** StatefulWidget.
**Capabilities:** A Material on/off toggle; controlled — the parent updates `value` from `onChanged`.
**Key constructors / named constructors:** `Switch({required value, required onChanged, ...})`, `Switch.adaptive(...)`.
**Key properties (constructor args):**
- `value`, `onChanged` (null disables).
- `activeColor`, `activeTrackColor`, `inactiveThumbColor`, `inactiveTrackColor`, `thumbColor`, `trackColor`, `trackOutlineColor`, `thumbIcon`, `activeThumbImage`, `inactiveThumbImage`, `materialTapTargetSize`, `dragStartBehavior`, `focusNode`, `autofocus`.
**Key methods:** `createState`.

### SwitchListTile
**Type:** StatelessWidget.
**Capabilities:** A `ListTile` combined with a `Switch`; tapping the row toggles the switch.
**Key constructors / named constructors:** `SwitchListTile({required value, required onChanged, ...})`, `.adaptive(...)`.
**Key properties (constructor args):**
- `value`, `onChanged`, `title`, `subtitle`, `secondary`, `controlAffinity`, `activeColor`, `activeTrackColor`, `inactiveThumbColor`, `inactiveTrackColor`, `tileColor`, `selected`, `dense`, `contentPadding`.
**Key methods:** `build`.

### Slider
**Type:** StatefulWidget.
**Capabilities:** A control for selecting a single continuous or discrete value by dragging a thumb along a track.
**Key constructors / named constructors:** `Slider({required value, required onChanged, ...})`, `Slider.adaptive(...)`.
**Key properties (constructor args):**
- `value`, `onChanged` (null disables), `onChangeStart`, `onChangeEnd`.
- `min`, `max`, `divisions`, `label`, `activeColor`, `inactiveColor`, `thumbColor`, `secondaryTrackValue`, `secondaryActiveColor`, `mouseCursor`, `focusNode`, `autofocus`.
**Key methods:** `createState`.

### RangeSlider
**Type:** StatefulWidget.
**Capabilities:** A slider with two thumbs selecting a range (`RangeValues`).
**Key constructors / named constructors:** `RangeSlider({required values, required onChanged, ...})`.
**Key properties (constructor args):**
- `values` (`RangeValues`), `onChanged` (null disables), `onChangeStart`, `onChangeEnd`, `min`, `max`, `divisions`, `labels` (`RangeLabels`), `activeColor`, `inactiveColor`, `overlayColor`, `mouseCursor`, `semanticFormatterCallback`.
**Key methods:** `createState`.

### Chip
**Type:** StatelessWidget (implements `ChipAttributes`, `DeletableChipAttributes`).
**Capabilities:** A compact element representing an attribute, entity, or action as a rounded label; the base, non-interactive chip (optionally deletable).
**Key constructors / named constructors:** `Chip({required label, ...})`.
**Key properties (constructor args):**
- `label`, `avatar`, `onDeleted`, `deleteIcon`, `deleteIconColor`, `deleteButtonTooltipMessage`.
- `backgroundColor`, `labelStyle`, `labelPadding`, `padding`, `shape`, `side`, `elevation`, `shadowColor`, `visualDensity`.
**Key methods:** `build`.

### InputChip
**Type:** StatelessWidget.
**Capabilities:** A chip representing a complex piece of information (e.g. a contact); can be selected, pressed, and deleted — useful for token entry.
**Key constructors / named constructors:** `InputChip({required label, ...})`.
**Key properties (constructor args):**
- `label`, `avatar`, `selected` + `onSelected`, `onPressed`, `onDeleted`, `deleteIcon`, `isEnabled`, `showCheckmark`, `selectedColor`, `backgroundColor`, `labelStyle`, `shape`, `side`, `pressElevation`, `avatarBorder`.
**Key methods:** `build`.

### ChoiceChip
**Type:** StatelessWidget.
**Capabilities:** A chip allowing single selection from a set (like a radio button in chip form).
**Key constructors / named constructors:** `ChoiceChip({required label, required selected, ...})`.
**Key properties (constructor args):**
- `label`, `avatar`, `selected`, `onSelected` (null disables), `selectedColor`, `disabledColor`, `backgroundColor`, `labelStyle`, `pressElevation`, `shape`, `side`, `showCheckmark`.
**Key methods:** `build`.

### FilterChip
**Type:** StatelessWidget.
**Capabilities:** A chip used to filter content via multiple selections (like a checkbox in chip form); shows a checkmark when selected.
**Key constructors / named constructors:** `FilterChip({required label, required onSelected, ...})`, `FilterChip.elevated(...)`.
**Key properties (constructor args):**
- `label`, `avatar`, `selected`, `onSelected` (null disables), `showCheckmark`, `checkmarkColor`, `selectedColor`, `backgroundColor`, `labelStyle`, `shape`, `side`, `pressElevation`.
**Key methods:** `build`.

### ActionChip
**Type:** StatelessWidget.
**Capabilities:** A chip that triggers an action when pressed (not a selection); a compact button styled as a chip.
**Key constructors / named constructors:** `ActionChip({required label, required onPressed, ...})`, `ActionChip.elevated(...)`.
**Key properties (constructor args):**
- `label`, `avatar`, `onPressed` (null disables), `pressElevation`, `backgroundColor`, `labelStyle`, `shape`, `side`, `tooltip`.
**Key methods:** `build`.

### Badge
**Type:** StatelessWidget, Material 3.
**Capabilities:** A small status descriptor (a dot or count label) overlaid on a child, typically on icons/avatars to indicate notifications or counts.
**Key constructors / named constructors:** `Badge({child, label, ...})`, `Badge.count({required count, ...})`.
**Key properties (constructor args):**
- `child`, `label`, `isLabelVisible`, `count` (`.count`), `backgroundColor`, `textColor`, `smallSize`, `largeSize`, `textStyle`, `padding`, `alignment`, `offset`.
**Key methods:** `build`.

### Dialog
**Type:** StatelessWidget.
**Capabilities:** A low-level Material dialog surface (rounded, elevated, centered) into which arbitrary content is placed; `AlertDialog`/`SimpleDialog` build on it.
**Key constructors / named constructors:** `Dialog({child, ...})`, `Dialog.fullscreen({required child, ...})`.
**Key properties (constructor args):**
- `child`, `backgroundColor`, `elevation`, `shadowColor`, `surfaceTintColor`, `insetPadding`, `insetAnimationDuration`, `insetAnimationCurve`, `shape`, `clipBehavior`, `alignment`.
**Key methods:** `build`. (Display via `showDialog`.)

### AlertDialog
**Type:** StatelessWidget.
**Capabilities:** A standard Material alert dialog with an optional title, content/message, and a row of action buttons.
**Key constructors / named constructors:** `AlertDialog({...})`, `AlertDialog.adaptive(...)`.
**Key properties (constructor args):**
- `title`, `content`, `actions`, `icon`, `iconColor`, `iconPadding`.
- `titlePadding`, `contentPadding`, `actionsPadding`, `actionsAlignment`, `actionsOverflowAlignment`, `backgroundColor`, `elevation`, `shape`, `insetPadding`, `scrollable`, `semanticLabel`.
**Key methods:** `build`. (Display via `showDialog`.)

### SimpleDialog
**Type:** StatelessWidget.
**Capabilities:** A Material dialog presenting a choice between several options listed vertically (typically `SimpleDialogOption`s).
**Key constructors / named constructors:** `SimpleDialog({...})`.
**Key properties (constructor args):**
- `title`, `children`, `titlePadding`, `contentPadding`, `insetPadding`, `backgroundColor`, `elevation`, `shape`, `alignment`, `semanticLabel`.
**Key methods:** `build`. (Display via `showDialog`.)

### SimpleDialogOption
**Type:** StatelessWidget.
**Capabilities:** A single tappable option row inside a `SimpleDialog`; commonly its `onPressed` calls `Navigator.pop` with the chosen value.
**Key constructors / named constructors:** `SimpleDialogOption({child, onPressed, ...})`.
**Key properties (constructor args):** `child`, `onPressed`, `padding`.
**Key methods:** `build`.

### showDialog
**Type:** Top-level function (`Future<T?> showDialog<T>({...})`).
**Capabilities:** Displays a modal Material dialog above the current content, dimming the background; returns a `Future` completing with the value passed to `Navigator.pop`.
**Key parameters:** `context`, `builder` (`WidgetBuilder`), `barrierDismissible`, `barrierColor`, `barrierLabel`, `useSafeArea`, `useRootNavigator`, `routeSettings`, `anchorPoint`, `traversalEdgeBehavior`. Related: `showGeneralDialog`.
**Key methods:** Returns `Future<T?>`.

### BottomSheet
**Type:** StatefulWidget.
**Capabilities:** A Material surface that slides up from the bottom; the low-level widget behind `showModalBottomSheet` and `Scaffold.bottomSheet`.
**Key constructors / named constructors:** `BottomSheet({required onClosing, required builder, ...})`.
**Key properties (constructor args):**
- `onClosing`, `builder`, `enableDrag`, `showDragHandle`, `dragHandleColor`, `dragHandleSize`, `animationController`, `backgroundColor`, `elevation`, `shape`, `clipBehavior`, `constraints`, `onDragStart`, `onDragEnd`.
**Key methods:** `createState`.

### showModalBottomSheet
**Type:** Top-level function (`Future<T?> showModalBottomSheet<T>({...})`).
**Capabilities:** Displays a modal (blocking) bottom sheet that slides up over the content with a scrim; returns a `Future`. Related: `showBottomSheet` (persistent, non-modal).
**Key parameters:** `context`, `builder` (required), `isScrollControlled`, `isDismissible`, `enableDrag`, `showDragHandle`, `useSafeArea`, `useRootNavigator`, `backgroundColor`, `elevation`, `shape`, `clipBehavior`, `constraints`, `barrierColor`, `barrierLabel`, `anchorPoint`, `routeSettings`, `transitionAnimationController`.
**Key methods:** Returns `Future<T?>`.

### showMenu
**Type:** Top-level function (`Future<T?> showMenu<T>({...})`).
**Capabilities:** Shows a Material popup menu at a given position (the mechanism behind `PopupMenuButton`); resolves with the selected value.
**Key parameters:** `context`, `position` (`RelativeRect`), `items` (`List<PopupMenuEntry<T>>`) (required); `initialValue`, `elevation`, `shape`, `color`, `surfaceTintColor`, `semanticLabel`, `constraints`, `clipBehavior`, `useRootNavigator`, `routeSettings`, `menuPadding`.
**Key methods:** Returns `Future<T?>`.

### Tooltip
**Type:** StatefulWidget.
**Capabilities:** Wraps a widget to show a short message on long-press (touch) or hover (mouse/desktop).
**Key constructors / named constructors:** `Tooltip({...})`.
**Key properties (constructor args):**
- `message` (or `richMessage`), `child`, `triggerMode`, `waitDuration`, `showDuration`, `preferBelow`, `verticalOffset`, `margin`, `padding`, `decoration`, `textStyle`, `textAlign`, `height`, `excludeFromSemantics`, `enableFeedback`.
**Key methods:** `createState`; `ensureTooltipVisible()`.

### Banner
**Type:** StatelessWidget.
**Capabilities:** Paints a diagonal ribbon across a corner of its child (the primitive behind the debug "DEBUG" banner). Distinct from `MaterialBanner`.
**Key constructors / named constructors:** `Banner({message, location, child, ...})`.
**Key properties (constructor args):** `message`, `location` (`BannerLocation`), `child`, `color`, `textStyle`, `textDirection`, `layoutDirection`.
**Key methods:** `build`.

### CircularProgressIndicator
**Type:** StatefulWidget.
**Capabilities:** A Material circular spinner showing indeterminate (spinning) or determinate (`value`) progress.
**Key constructors / named constructors:** `CircularProgressIndicator({...})`, `.adaptive(...)`.
**Key properties (constructor args):**
- `value` (`double?`; null = indeterminate), `strokeWidth`, `strokeAlign`, `strokeCap`, `color`, `backgroundColor`, `valueColor`, `semanticsLabel`, `semanticsValue`.
**Key methods:** `createState`.

### LinearProgressIndicator
**Type:** StatefulWidget.
**Capabilities:** A Material horizontal bar showing indeterminate or determinate progress.
**Key constructors / named constructors:** `LinearProgressIndicator({...})`.
**Key properties (constructor args):**
- `value` (`double?`), `minHeight`, `color`, `backgroundColor`, `valueColor`, `borderRadius`, `semanticsLabel`, `semanticsValue`.
**Key methods:** `createState`.

### RefreshProgressIndicator
**Type:** StatefulWidget (subclass of `CircularProgressIndicator`).
**Capabilities:** The circular spinner shown inside a `RefreshIndicator`, rendered on a small elevated disc.
**Key constructors / named constructors:** `RefreshProgressIndicator({...})`.
**Key properties (constructor args):** `value`, `strokeWidth`, `color`, `backgroundColor`, `valueColor`, `elevation`, `indicatorMargin`, `indicatorPadding`, `semanticsLabel`, `semanticsValue`.
**Key methods:** `createState`.

### Divider
**Type:** StatelessWidget.
**Capabilities:** A thin horizontal rule (with surrounding vertical padding) to separate content.
**Key constructors / named constructors:** `Divider({...})`.
**Key properties (constructor args):** `height`, `thickness`, `indent`, `endIndent`, `color`, `radius`.
**Key methods:** `build`; `static createBorderSide(context, {color, width})`.

### VerticalDivider
**Type:** StatelessWidget.
**Capabilities:** A thin vertical rule (with horizontal padding) separating content laid out in a row.
**Key constructors / named constructors:** `VerticalDivider({...})`.
**Key properties (constructor args):** `width`, `thickness`, `indent`, `endIndent`, `color`.
**Key methods:** `build`.

### DataTable
**Type:** StatefulWidget.
**Capabilities:** A Material data table with sortable headers, optional row selection (checkboxes), and fixed styling; suited to moderate, non-paged data sets.
**Key constructors / named constructors:** `DataTable({required columns, required rows, ...})`.
**Key properties (constructor args):**
- `columns` (`List<DataColumn>`), `rows` (`List<DataRow>`), `sortColumnIndex`, `sortAscending`, `onSelectAll`.
- `showCheckboxColumn`, `dataRowMinHeight`, `dataRowMaxHeight`, `headingRowHeight`, `columnSpacing`, `horizontalMargin`, `dividerThickness`, `border`, `decoration`, `showBottomBorder`, `headingRowColor`, `dataRowColor`, `checkboxHorizontalMargin`.
**Key methods:** `createState`.

### DataColumn
**Type:** Class (immutable config, not a widget).
**Capabilities:** Describes a single `DataTable` column — its header label, alignment, and sort behavior.
**Key constructors / named constructors:** `DataColumn({required label, ...})`.
**Key properties (constructor args):** `label`, `tooltip`, `numeric`, `onSort` (`DataColumnSortCallback?`), `columnWidth`, `headingRowAlignment`.
**Key methods:** N/A.

### DataRow
**Type:** Class (immutable config, not a widget).
**Capabilities:** Describes one row of a `DataTable`: its `DataCell`s plus optional selection state and per-row callbacks.
**Key constructors / named constructors:** `DataRow({required cells, ...})`, `DataRow.byIndex({required index, required cells, ...})`.
**Key properties (constructor args):** `cells`, `selected`, `onSelectChanged`, `onLongPress`, `color`, `mouseCursor`.
**Key methods:** N/A.

### DataCell
**Type:** Class (immutable config, not a widget).
**Capabilities:** Describes the content of a single cell within a `DataRow`, plus optional interaction callbacks and a placeholder state.
**Key constructors / named constructors:** `DataCell(Widget child, {...})`.
**Key properties (constructor args):** `child`, `placeholder`, `showEditIcon`, `onTap`, `onLongPress`, `onDoubleTap`, `onTapDown`, `onTapCancel`. Constant `DataCell.empty`.
**Key methods:** N/A.

### PaginatedDataTable
**Type:** StatefulWidget.
**Capabilities:** A `DataTable` variant that displays data in pages, pulling rows lazily from a `DataTableSource` with page navigation and a rows-per-page selector.
**Key constructors / named constructors:** `PaginatedDataTable({required columns, required source, ...})`.
**Key properties (constructor args):**
- `columns`, `source`, `header`, `actions`, `rowsPerPage`, `availableRowsPerPage`, `onRowsPerPageChanged`, `initialFirstRowIndex`, `onPageChanged`, `sortColumnIndex`, `sortAscending`, `onSelectAll`, `columnSpacing`, `horizontalMargin`, `showCheckboxColumn`, `showFirstLastButtons`, `dataRowMinHeight`, `dataRowMaxHeight`, `headingRowHeight`.
**Key methods:** `createState`.

### Stepper
**Type:** StatefulWidget.
**Capabilities:** Displays progress through a sequence of numbered `Step`s (vertical or horizontal) with continue/cancel controls.
**Key constructors / named constructors:** `Stepper({required steps, ...})`.
**Key properties (constructor args):**
- `steps`, `currentStep`, `type` (`StepperType`), `onStepTapped`, `onStepContinue`, `onStepCancel`, `controlsBuilder`, `physics`, `elevation`, `margin`, `connectorColor`, `connectorThickness`, `stepIconBuilder`.
**Key methods:** `createState`.

### Step
**Type:** Class (immutable config, not a widget).
**Capabilities:** Describes one step in a `Stepper`: title, optional subtitle, content, and state.
**Key constructors / named constructors:** `Step({required title, required content, ...})`.
**Key properties (constructor args):** `title`, `subtitle`, `content`, `state` (`StepState`), `isActive`, `label`, `stepStyle`.
**Key methods:** N/A.

### CircleAvatar
**Type:** StatelessWidget.
**Capabilities:** A circle representing a user — typically a profile image, initials, or icon; sizes itself to `radius`.
**Key constructors / named constructors:** `CircleAvatar({...})`.
**Key properties (constructor args):**
- `child`, `backgroundImage`, `foregroundImage` (with `onBackgroundImageError`/`onForegroundImageError`), `backgroundColor`, `foregroundColor`, `radius`, `minRadius`, `maxRadius`.
**Key methods:** `build`.

---

## Text & input

### Text
**Type:** StatelessWidget.
**Capabilities:** Displays a run of styled, read-only text using a single style. The most common widget for showing strings; supports overflow handling, soft wrapping, max line limits, and scaling.
**Key constructors / named constructors:**
- `Text(String data, {...})` — plain string.
- `Text.rich(InlineSpan textSpan, {...})` — a tree of spans (`TextSpan`/`WidgetSpan`) for mixed styling.
**Key properties (constructor args):**
- `data`, `style` (merged onto ambient `DefaultTextStyle` unless `style.inherit` is false).
- `textAlign`, `textDirection`, `overflow` (`clip`/`fade`/`ellipsis`/`visible`), `softWrap`, `maxLines`.
- `textScaler` (replaces deprecated `textScaleFactor`), `semanticsLabel`, `locale`, `strutStyle`, `textWidthBasis`, `textHeightBehavior`, `selectionColor`.
**Key methods:** `build` (composes an underlying `RichText`).

### Text.rich
**Type:** Named constructor of `Text`.
**Capabilities:** Builds a `Text` from an `InlineSpan` tree, allowing multiple styles, inline gestures, and inline widgets (`WidgetSpan`).
**Key constructors / named constructors:** `Text.rich(InlineSpan textSpan, {...})`.
**Key properties (constructor args):** `textSpan` (first positional); shares all styling/layout properties with `Text`.
**Key methods:** `build` (composes a `RichText`).

### RichText
**Type:** MultiChildRenderObjectWidget (low-level).
**Capabilities:** Renders an `InlineSpan` tree directly without consulting `DefaultTextStyle`; the primitive that `Text` builds on. Use for full control with all styles explicit.
**Key constructors / named constructors:** `RichText({required InlineSpan text, ...})`.
**Key properties (constructor args):**
- `text` (required; every span must carry an explicit style), `textAlign`, `textDirection`, `softWrap`, `overflow`, `textScaler`, `maxLines`, `locale`, `strutStyle`, `textWidthBasis`, `textHeightBehavior`, `selectionRegistrar`, `selectionColor`.
**Key methods:** `createRenderObject` / `updateRenderObject` (produces a `RenderParagraph`).

### TextSpan (supporting)
**Type:** `InlineSpan` subclass (immutable data class, not a widget).
**Capabilities:** An immutable span of text with a `TextStyle`, optional `children`, an optional `recognizer` for gestures, and accessibility/mouse-cursor metadata. The building block of rich text trees.
**Key constructors / named constructors:** `TextSpan({String? text, List<InlineSpan>? children, TextStyle? style, GestureRecognizer? recognizer, ...})`.
**Key properties (constructor args):**
- `text`, `children`, `style`, `recognizer` (e.g. `TapGestureRecognizer`; owner must dispose), `mouseCursor`, `onEnter`, `onExit`, `semanticsLabel`, `locale`, `spellOut`.
**Key methods:** `build`, `visitChildren`, `computeToPlainText`, `getSpanForPosition`.

### WidgetSpan
**Type:** `PlaceholderSpan` / `InlineSpan` subclass.
**Capabilities:** Embeds an arbitrary widget inline within a run of text, participating in text layout as a placeholder.
**Key constructors / named constructors:** `WidgetSpan({required Widget child, PlaceholderAlignment alignment, TextBaseline? baseline, TextStyle? style, ...})`; `WidgetSpan.fromWidgetSpans(...)`.
**Key properties (constructor args):**
- `child` (required), `alignment` (`PlaceholderAlignment`), `baseline` (required for baseline alignment), `style`.
**Key methods:** `build`, `visitChildren`, `codeUnitAtVisitor`.

### DefaultTextStyle
**Type:** InheritedWidget.
**Capabilities:** Provides an ambient `TextStyle` (and text layout defaults) to descendant `Text` widgets, which merge their own style onto it.
**Key constructors / named constructors:** `DefaultTextStyle({required TextStyle style, required Widget child, ...})`; `DefaultTextStyle.merge({...})`; `DefaultTextStyle.fallback()`.
**Key properties (constructor args):**
- `style`, `textAlign`, `softWrap`, `overflow`, `maxLines`, `textWidthBasis`, `textHeightBehavior`, `child`.
**Key methods:** `static DefaultTextStyle of(BuildContext)`; `updateShouldNotify`.

### SelectableText
**Type:** StatefulWidget.
**Capabilities:** Displays read-only text the user can select and copy without making it editable, with optional cursor, toolbar, and tap/long-press callbacks. Roughly a read-only `TextField`.
**Key constructors / named constructors:** `SelectableText(String data, {...})`; `SelectableText.rich(TextSpan textSpan, {...})`.
**Key properties (constructor args):**
- `data`/`textSpan`, `style`, `textAlign`, `textDirection`, `maxLines`, `textScaler`, `strutStyle`, `focusNode`.
- `showCursor`, `cursorWidth`, `cursorColor`, `cursorRadius`, `onTap`, `onSelectionChanged`, `selectionControls`, `contextMenuBuilder`, `magnifierConfiguration`, `scrollPhysics`, `semanticsLabel`.
**Key methods:** `createState` (manages selection, focus, gestures).

### SelectableText.rich
**Type:** Named constructor of `SelectableText`.
**Capabilities:** Selectable behavior driven by a `TextSpan` tree, enabling mixed styles and inline gesture recognizers in selectable read-only text.
**Key constructors / named constructors:** `SelectableText.rich(TextSpan textSpan, {...})`.
**Key properties (constructor args):** `textSpan` (first positional); shares cursor/selection/interaction properties with `SelectableText`.
**Key methods:** `createState`.

### SelectionArea
**Type:** StatefulWidget.
**Capabilities:** Makes a subtree of otherwise non-selectable widgets (e.g. ordinary `Text`) selectable as one cohesive region, providing native selection gestures, toolbar, and copy support. Built on `SelectableRegion`.
**Key constructors / named constructors:** `SelectionArea({required Widget child, ...})`.
**Key properties (constructor args):**
- `child`, `focusNode`, `selectionControls`, `contextMenuBuilder`, `magnifierConfiguration`, `onSelectionChanged`.
**Key methods:** `createState` (wraps a `SelectableRegion`).

### DefaultTextHeightBehavior
**Type:** InheritedWidget.
**Capabilities:** Provides an ambient `TextHeightBehavior` (controlling how leading/height applies to first ascent and last descent) to descendant text widgets.
**Key constructors / named constructors:** `DefaultTextHeightBehavior({required TextHeightBehavior textHeightBehavior, required Widget child, ...})`.
**Key properties (constructor args):** `textHeightBehavior`, `child`.
**Key methods:** `static TextHeightBehavior? of(BuildContext)`; `updateShouldNotify`.

### TextField
**Type:** StatefulWidget (Material).
**Capabilities:** The standard Material single- or multi-line editable text input, wrapping `EditableText` with Material decoration, selection toolbar, cursor, and focus handling. Reports changes via callbacks rather than form validation.
**Key constructors / named constructors:** `TextField({...})`.
**Key properties (constructor args):**
- `controller`, `focusNode`, `decoration` (`InputDecoration`; null = none).
- `keyboardType`, `textInputAction`, `onChanged`, `onEditingComplete`, `onSubmitted`.
- `obscureText`, `obscuringCharacter`, `inputFormatters`, `maxLines`, `minLines`, `expands`, `maxLength`, `maxLengthEnforcement`.
- `readOnly`, `enabled`, `autofocus`, `autocorrect`, `enableSuggestions`, `style`, `textAlign`, `textCapitalization`, `cursorColor`, `cursorWidth`, `contextMenuBuilder`, `magnifierConfiguration`, `onTapOutside`.
**Key methods:** `createState` (manages controller/focus lifecycle and builds the `EditableText`).

### TextFormField
**Type:** StatefulWidget (a `FormField<String>` wrapper, Material).
**Capabilities:** A `TextField` integrated with `Form`/`FormField` so it participates in form-wide save, validation, and reset.
**Key constructors / named constructors:** `TextFormField({...})`.
**Key properties (constructor args):**
- `controller`/`initialValue` (provide one), passthroughs (`decoration`, `keyboardType`, `obscureText`, `inputFormatters`, `maxLines`, ...).
- `validator` (`String? Function(String?)`), `onSaved`, `onChanged`, `onFieldSubmitted`, `autovalidateMode`, `restorationId`.
**Key methods:** `createState` → `FormFieldState<String>` (`didChange`, `validate`, `save`, `reset`).

### Form
**Type:** StatefulWidget.
**Capabilities:** Groups multiple `FormField` descendants so they can be saved, validated, and reset together, and optionally guards navigation with change detection.
**Key constructors / named constructors:** `Form({required Widget child, ...})`.
**Key properties (constructor args):**
- `child`, `key` (typically `GlobalKey<FormState>`), `onChanged`, `autovalidateMode`, `canPop` / `onPopInvokedWithResult` (replacing the older `onWillPop`).
**Key methods:** `static FormState? of(BuildContext)`; `createState` → `FormState`.

### FormField
**Type:** StatefulWidget (generic `FormField<T>`).
**Capabilities:** Base class for a single form control maintaining a value of type `T`, validation error state, and registration with the enclosing `Form`.
**Key constructors / named constructors:** `FormField<T>({required FormFieldBuilder<T> builder, ...})`.
**Key properties (constructor args):**
- `builder` (`Widget Function(FormFieldState<T>)`), `initialValue`, `validator`, `onSaved`, `autovalidateMode`, `enabled`, `restorationId`, `forceErrorText`.
**Key methods:** `createState` → `FormFieldState<T>`.

### FormState
**Type:** `State<Form>` (the State object for `Form`).
**Capabilities:** The imperative controller for a `Form`; iterates registered fields to validate, save, and reset, and tracks dirty state. Obtained via a `GlobalKey<FormState>` or `Form.of(context)`.
**Key methods:**
- `bool validate()` — runs every field's validator.
- `void save()` — calls every field's `onSaved`.
- `void reset()` — restores every field to its initial value.
- `validateGranularly()` — validates and returns the set of invalid fields.

### EditableText (low-level)
**Type:** StatefulWidget (low-level).
**Capabilities:** The core text-editing primitive underlying `TextField`, `TextFormField`, `SelectableText`, and `CupertinoTextField`: connects to the platform text input, manages editing value/cursor/selection, and renders the editable text. Prefer the higher-level fields.
**Key constructors / named constructors:** `EditableText({required TextEditingController controller, required FocusNode focusNode, required TextStyle style, required Color cursorColor, required Color backgroundCursorColor, ...})`.
**Key properties (constructor args):** `controller`, `focusNode`, `style`, `cursorColor`, `backgroundCursorColor` (required); `keyboardType`, `textInputAction`, `obscureText`, `maxLines`, `minLines`, `inputFormatters`, `onChanged`, `onSubmitted`, `selectionColor`, `showCursor`, etc.
**Key methods:** `createState` → `EditableTextState` (`copySelection`, `cutSelection`, `pasteText`, `toggleToolbar`, plus the `TextSelectionDelegate`/`TextInputClient` machinery).

### TextEditingController (supporting)
**Type:** `ValueNotifier<TextEditingValue>` (a `ChangeNotifier`, not a widget).
**Capabilities:** Holds the current text, selection, and composing range of an editable field and notifies listeners on change.
**Key constructors / named constructors:** `TextEditingController({String? text})`; `TextEditingController.fromValue(TextEditingValue value)`.
**Key properties:** `text`, `value`, `selection`.
**Key methods:** `clear()`, `clearComposing()`, `buildTextSpan(...)`, plus `addListener`/`dispose`.

### FocusNode (supporting)
**Type:** `ChangeNotifier` (not a widget).
**Capabilities:** Represents a focusable entry in the focus tree; query and request focus, listen for focus/attachment changes, configure key handling and traversal.
**Key constructors / named constructors:** `FocusNode({String? debugLabel, FocusOnKeyEventCallback? onKeyEvent, bool skipTraversal, bool canRequestFocus, bool descendantsAreFocusable, ...})`.
**Key properties:** `hasFocus`, `hasPrimaryFocus`, `canRequestFocus`, `skipTraversal`, `descendantsAreFocusable`, `onKeyEvent`.
**Key methods:** `requestFocus()`, `unfocus({UnfocusDisposition})`, `nextFocus()`, `previousFocus()`, `dispose()`.

### Focus
**Type:** StatefulWidget.
**Capabilities:** Wraps a subtree to make it focusable, managing (or accepting) a `FocusNode`, handling key events, and exposing focus state via `Focus.of`.
**Key constructors / named constructors:** `Focus({required Widget child, ...})`; `Focus.withExternalFocusNode({required Widget child, required FocusNode focusNode, ...})`.
**Key properties (constructor args):** `child`, `focusNode`, `autofocus`, `onFocusChange`, `onKeyEvent`, `canRequestFocus`, `skipTraversal`, `descendantsAreFocusable`.
**Key methods:** `static FocusNode of(BuildContext)`; `static bool? maybeOf(...)`; `createState`.

### FocusScope
**Type:** StatefulWidget (a specialized `Focus`).
**Capabilities:** Establishes a focus scope — a grouping that retains the last focused child and bounds focus traversal — for pages, dialogs, and logical groups.
**Key constructors / named constructors:** `FocusScope({required Widget child, ...})`; `FocusScope.withExternalFocusNode({required Widget child, required FocusScopeNode focusScopeNode, ...})`.
**Key properties (constructor args):** `child`, `node` (`FocusScopeNode`), `autofocus`, `onFocusChange`, `canRequestFocus`, `skipTraversal`, `onKeyEvent`.
**Key methods:** `static FocusScopeNode of(BuildContext)`; `static FocusScopeNode? maybeOf(...)`; `createState`.

### FocusTraversalGroup
**Type:** StatefulWidget.
**Capabilities:** Groups descendant focusable widgets so Tab/arrow traversal orders them together according to a policy, before moving on.
**Key constructors / named constructors:** `FocusTraversalGroup({required Widget child, FocusTraversalPolicy? policy, ...})`.
**Key properties (constructor args):** `child`, `policy` (`WidgetOrderTraversalPolicy`, `ReadingOrderTraversalPolicy`, `OrderedTraversalPolicy`), `descendantsAreFocusable`, `descendantsAreTraversable`.
**Key methods:** `static FocusTraversalPolicy of(BuildContext)`; `createState`.

### Autocomplete
**Type:** StatelessWidget.
**Capabilities:** A ready-made autocomplete/typeahead widget showing a field and an overlay of filtered options; a Material-styled wrapper around `RawAutocomplete`.
**Key constructors / named constructors:** `Autocomplete<T extends Object>({required AutocompleteOptionsBuilder<T> optionsBuilder, ...})`.
**Key properties (constructor args):** `optionsBuilder`, `displayStringForOption`, `onSelected`, `fieldViewBuilder`, `optionsViewBuilder`, `initialValue`, `optionsMaxHeight`.
**Key methods:** `build` (composes a `RawAutocomplete`).

### RawAutocomplete
**Type:** StatefulWidget.
**Capabilities:** The unstyled, fully customizable foundation for autocomplete: you supply field and options view builders; it manages filtering, overlay, highlight/keyboard navigation, and selection.
**Key constructors / named constructors:** `RawAutocomplete<T extends Object>({required AutocompleteOptionsBuilder<T> optionsBuilder, required AutocompleteOptionsViewBuilder<T> optionsViewBuilder, ...})`.
**Key properties (constructor args):** `optionsBuilder`, `optionsViewBuilder` (required), `fieldViewBuilder`, `displayStringForOption`, `onSelected`, `textEditingController` + `focusNode` (together if external), `initialValue`, `optionsViewOpenDirection`.
**Key methods:** `static String defaultStringForOption(dynamic option)`; `createState`.

### CupertinoTextField (cross-ref)
**Type:** StatefulWidget (Cupertino).
**Capabilities:** The iOS-styled counterpart to `TextField`, also built on `EditableText`, with Cupertino decoration (`BoxDecoration`) instead of `InputDecoration`. See `TextField` for shared editing properties.
**Key constructors / named constructors:** `CupertinoTextField({...})`; `CupertinoTextField.borderless({...})`.
**Key properties (constructor args):** `controller`, `focusNode`, `keyboardType`, `obscureText`, `onChanged`, `onSubmitted`, `inputFormatters`, `maxLines` (shared); `decoration` (`BoxDecoration`), `placeholder`, `prefix`, `suffix`, `clearButtonMode`, `padding`.
**Key methods:** `createState`.

### InputDecorator
**Type:** StatefulWidget.
**Capabilities:** Paints an `InputDecoration` (labels, hints, icons, borders, helper/error text, counter) around a child — typically the editable area of a `TextField`. Can give a non-text widget the Material text-field look.
**Key constructors / named constructors:** `InputDecorator({required InputDecoration decoration, Widget? child, ...})`.
**Key properties (constructor args):**
- `decoration` (required), `child`, `baseStyle`, `textAlign`, `textAlignVertical`, `isFocused`, `isHovering`, `isEmpty`, `expands`.
**Key methods:** `createState` (animates label/border transitions).

### InputDecoration (supporting class)
**Type:** Immutable configuration class (not a widget).
**Capabilities:** Describes the decorative chrome of a Material text field — labels, hints, helper/error text, icons, prefixes/suffixes, borders, fill, and constraints — consumed by `InputDecorator`/`TextField`.
**Key constructors / named constructors:** `InputDecoration({...})`; `InputDecoration.collapsed({required String? hintText, ...})`.
**Key properties (fields):**
- `labelText`/`label`, `floatingLabelBehavior`, `floatingLabelStyle`.
- `hintText`/`hintStyle`, `helperText`, `errorText`, `errorMaxLines`.
- `icon`, `prefixIcon`, `suffixIcon`, `prefix`, `suffix`, `prefixText`, `suffixText`.
- `border`, `enabledBorder`, `focusedBorder`, `errorBorder`, `disabledBorder` (`OutlineInputBorder`/`UnderlineInputBorder`).
- `filled`, `fillColor`, `contentPadding`, `isDense`, `isCollapsed`, `constraints`, `counterText`/`counter`, `enabled`.
**Key methods:** `copyWith(...)`, `applyDefaults(InputDecorationTheme)`.

### KeyboardListener
**Type:** StatefulWidget.
**Capabilities:** Calls a callback for raw `KeyEvent`s whenever its `FocusNode` has focus, using the modern `HardwareKeyboard`/`KeyEvent` API. The non-deprecated replacement for `RawKeyboardListener`.
**Key constructors / named constructors:** `KeyboardListener({required FocusNode focusNode, required Widget child, ValueChanged<KeyEvent>? onKeyEvent, ...})`.
**Key properties (constructor args):** `focusNode` (required), `onKeyEvent`, `autofocus`, `includeSemantics`, `child` (required).
**Key methods:** `createState`.

### RawKeyboardListener
**Type:** StatefulWidget (legacy).
**Capabilities:** Older equivalent of `KeyboardListener` delivering `RawKeyEvent`s via the deprecated `RawKeyboard` API. Prefer `KeyboardListener`.
**Key constructors / named constructors:** `RawKeyboardListener({required FocusNode focusNode, required Widget child, ValueChanged<RawKeyEvent>? onKey, ...})`.
**Key properties (constructor args):** `focusNode` (required), `onKey`, `child`, `includeSemantics`.
**Key methods:** `createState`.

### Shortcuts
**Type:** StatefulWidget.
**Capabilities:** Maps key combinations (`ShortcutActivator`/`LogicalKeySet`) to `Intent`s for its subtree; a bound chord pressed while focused dispatches the corresponding `Intent` up to the `Actions` layer.
**Key constructors / named constructors:** `Shortcuts({required Map<ShortcutActivator, Intent> shortcuts, required Widget child, ...})`; `Shortcuts.manager({required ShortcutManager manager, ...})`.
**Key properties (constructor args):** `shortcuts` (`SingleActivator`, `LogicalKeySet`), `manager`, `child`, `debugLabel`.
**Key methods:** `static ShortcutManager? of(...)`; `createState`.

### Actions
**Type:** StatefulWidget.
**Capabilities:** Provides a map from `Intent` types to `Action` objects for its subtree; an invoked `Intent` is handled by the nearest matching enabled `Action`. The dispatch half of the Shortcuts/Actions pattern.
**Key constructors / named constructors:** `Actions({required Map<Type, Action<Intent>> actions, required Widget child, ...})`.
**Key properties (constructor args):** `actions` (keyed by `Intent` subtype), `dispatcher`, `child`.
**Key methods:** `static Object? invoke<T extends Intent>(BuildContext, T)`; `static Object? maybeInvoke<T>(...)`; `static ActionDispatcher of(BuildContext)`; `createState`.

### Intent / Action (supporting)
**Type:** `Intent` — immutable marker class; `Action<T extends Intent>` — `ChangeNotifier`-based handler. Neither is a widget.
**Capabilities:** An `Intent` declaratively describes an operation (`ActivateIntent`, `DismissIntent`, custom intents carrying data); an `Action` executes when its `Intent` is invoked and may be enabled/disabled. Together they decouple key bindings/command sources from behavior.
**Key constructors / named constructors:** `const Intent()`; `Action<T>` (extend it); `CallbackAction<T>({required OnInvokeCallback<T> onInvoke})`.
**Key properties:** `Action.isEnabled(T)` / `isActionEnabled`; `CallbackAction.onInvoke`.
**Key methods:** `Object? invoke(T intent)` (the core behavior), `consumesKey`, `addActionListener`.

### CallbackShortcuts
**Type:** StatelessWidget.
**Capabilities:** A lightweight alternative to `Shortcuts`/`Actions` mapping key activators directly to callbacks, skipping the `Intent`/`Action` indirection.
**Key constructors / named constructors:** `CallbackShortcuts({required Map<ShortcutActivator, VoidCallback> bindings, required Widget child, ...})`.
**Key properties (constructor args):** `bindings`, `child`.
**Key methods:** `build` (wraps the child in a `Focus` that handles the bound keys).

---

## Scrolling & slivers

### ListView
**Type:** StatelessWidget (a box widget wrapping a scrollable list of linearly-arranged children).
**Capabilities:** A scrollable, linearly-arranged list of children along a single axis. The most common scrolling widget; internally backed by a `CustomScrollView` with a single sliver.
**Key constructors / named constructors:**
- `ListView()` — eagerly builds all `children`; for small finite lists.
- `ListView.builder()` — lazily builds via `itemBuilder`/`itemCount`; for large/infinite lists.
- `ListView.separated()` — like `.builder` plus a `separatorBuilder` between items.
- `ListView.custom()` — uses a `SliverChildDelegate`.
**Key properties (constructor args):**
- `scrollDirection`, `reverse`, `controller`, `primary`, `physics`, `shrinkWrap`, `padding`.
- `itemExtent`, `prototypeItem` (perf optimizations).
- `itemBuilder`/`itemCount`/`separatorBuilder`, `childrenDelegate`.
- `cacheExtent`, `addAutomaticKeepAlives`, `addRepaintBoundaries`, `addSemanticIndexes`, `keyboardDismissBehavior`, `restorationId`, `clipBehavior`.
**Key methods:** `build`, `buildChildLayout`, `buildSlivers` (inherited from `BoxScrollView`/`ScrollView`).

### GridView
**Type:** StatelessWidget (a scrollable 2D array of children).
**Capabilities:** A scrollable grid placing children in a 2D arrangement governed by a `SliverGridDelegate`. Backed by a `CustomScrollView` with a single `SliverGrid`.
**Key constructors / named constructors:** `GridView()`, `.count()`, `.extent()`, `.builder()`, `.custom()`.
**Key properties (constructor args):**
- `gridDelegate` (required for default/`.builder`/`.custom`), `crossAxisCount` (`.count`), `maxCrossAxisExtent` (`.extent`), `mainAxisSpacing`, `crossAxisSpacing`, `childAspectRatio`.
- `scrollDirection`, `reverse`, `controller`, `primary`, `physics`, `shrinkWrap`, `padding`, `itemBuilder`/`itemCount`, `childrenDelegate`, `cacheExtent`, `clipBehavior`, `restorationId`, `keyboardDismissBehavior`.
**Key methods:** `build`, `buildChildLayout` (returns a `SliverGrid`), `buildSlivers`.

### SliverGridDelegateWithFixedCrossAxisCount
**Type:** SliverGridDelegate (layout delegate, not a widget).
**Capabilities:** Lays out grid tiles with a fixed number of tiles in the cross axis.
**Key constructors / named constructors:** `SliverGridDelegateWithFixedCrossAxisCount({required crossAxisCount, ...})`.
**Key properties (constructor args):** `crossAxisCount` (required), `mainAxisSpacing`, `crossAxisSpacing`, `childAspectRatio`, `mainAxisExtent`.
**Key methods:** `getLayout(SliverConstraints)` → `SliverGridLayout`; `shouldRelayout(oldDelegate)`.

### SliverGridDelegateWithMaxCrossAxisExtent
**Type:** SliverGridDelegate.
**Capabilities:** Lays out tiles each at most `maxCrossAxisExtent`, choosing the tile count to fill the cross axis evenly. Good for responsive grids.
**Key constructors / named constructors:** `SliverGridDelegateWithMaxCrossAxisExtent({required maxCrossAxisExtent, ...})`.
**Key properties (constructor args):** `maxCrossAxisExtent` (required), `mainAxisSpacing`, `crossAxisSpacing`, `childAspectRatio`, `mainAxisExtent`.
**Key methods:** `getLayout(SliverConstraints)`; `shouldRelayout(oldDelegate)`.

### PageView
**Type:** StatefulWidget.
**Capabilities:** A scrollable list whose children each fill the viewport, snapping to one page per gesture. Driven by a `PageController`.
**Key constructors / named constructors:** `PageView()`, `.builder()`, `.custom()`.
**Key properties (constructor args):**
- `controller` (`PageController`: `initialPage`, `viewportFraction`), `scrollDirection`, `reverse`, `physics`, `pageSnapping`, `onPageChanged`, `padEnds`, `allowImplicitScrolling`, `itemBuilder`/`itemCount`, `childrenDelegate`, `clipBehavior`, `restorationId`, `scrollBehavior`.
**Key methods:** `createState`.

### CustomScrollView
**Type:** StatelessWidget (a `ScrollView` with directly-supplied slivers).
**Capabilities:** Composes scrolling effects from an arbitrary list of sliver children, enabling mixed content like floating app bars, lists, and grids in one viewport.
**Key constructors / named constructors:** `CustomScrollView({slivers, ...})`.
**Key properties (constructor args):**
- `slivers`, `scrollDirection`, `reverse`, `controller`, `primary`, `physics`, `shrinkWrap`, `center`, `anchor`, `cacheExtent`, `semanticChildCount`, `clipBehavior`, `keyboardDismissBehavior`, `restorationId`.
**Key methods:** `buildSlivers` (returns `slivers`), `build`.

### NestedScrollView
**Type:** StatefulWidget.
**Capabilities:** Coordinates an outer scroll view (e.g. a `SliverAppBar`) with an inner scrollable (e.g. tabbed `ListView`s), so a shared header collapses/expands as the body scrolls.
**Key constructors / named constructors:** `NestedScrollView({required headerSliverBuilder, required body, ...})`.
**Key properties (constructor args):** `headerSliverBuilder`, `body`, `controller`, `scrollDirection`, `reverse`, `physics`, `floatHeaderSlivers`, `clipBehavior`, `dragStartBehavior`, `restorationId`.
**Key methods:** `createState` → `NestedScrollViewState`; static `NestedScrollView.sliverOverlapAbsorberHandleFor(context)`.

### SingleChildScrollView
**Type:** StatelessWidget.
**Capabilities:** Makes a single child scrollable when it would overflow along one axis. Does not lazily build (the whole child is laid out).
**Key constructors / named constructors:** `SingleChildScrollView({child, ...})`.
**Key properties (constructor args):** `scrollDirection`, `reverse`, `controller`, `primary`, `physics`, `padding`, `child`, `clipBehavior`, `dragStartBehavior`, `keyboardDismissBehavior`, `restorationId`.
**Key methods:** `build`.

### Scrollable
**Type:** StatefulWidget (the low-level primitive underlying all scroll views).
**Capabilities:** Implements the interaction (gestures, physics, position) for scrolling a single `Viewport`. Higher-level widgets build a `Scrollable` internally.
**Key constructors / named constructors:** `Scrollable({required viewportBuilder, ...})`.
**Key properties (constructor args):** `viewportBuilder`, `axisDirection`, `controller`, `physics`, `excludeFromSemantics`, `semanticChildCount`, `dragStartBehavior`, `restorationId`, `scrollBehavior`, `clipBehavior`, `hitTestBehavior`.
**Key methods:** `createState` → `ScrollableState`; static `Scrollable.of(context)`, `Scrollable.ensureVisible(context, ...)`, `Scrollable.recommendDeferredLoadingForContext(context)`.

### Scrollbar
**Type:** StatefulWidget (Material scrollbar).
**Capabilities:** Adds a scrollbar thumb to a descendant scrollable, indicating position and (optionally) allowing drag-to-scroll. Cupertino has `CupertinoScrollbar`; `RawScrollbar` is the base.
**Key constructors / named constructors:** `Scrollbar({required child, ...})`.
**Key properties (constructor args):** `child`, `controller` (required when not the primary scroll view), `thumbVisibility`, `trackVisibility`, `thickness`, `radius`, `interactive`, `scrollbarOrientation`, `notificationPredicate`.
**Key methods:** `createState`.

### ScrollConfiguration
**Type:** InheritedWidget.
**Capabilities:** Provides a `ScrollBehavior` to descendant scrollables, controlling platform-dependent defaults like physics, overscroll glow/indicators, scrollbars, and accepted input devices.
**Key constructors / named constructors:** `ScrollConfiguration({required behavior, required child})`.
**Key properties (constructor args):** `behavior`, `child`.
**Key methods:** `static ScrollConfiguration.of(context)` → `ScrollBehavior`; `updateShouldNotify`. `ScrollBehavior` overridables: `getScrollPhysics`, `buildScrollbar`, `buildOverscrollIndicator`, `copyWith`, `dragDevices`.

### ScrollNotification + listeners
**Type:** Notification subclasses (`ScrollNotification` hierarchy).
**Capabilities:** Scrollables dispatch `Notification`s describing scroll activity: `ScrollStartNotification`, `ScrollUpdateNotification`, `ScrollEndNotification`, `OverscrollNotification`, `UserScrollNotification`. Listen via `NotificationListener<ScrollNotification>`.
**Key properties (constructor args):** `metrics` (`ScrollMetrics`: pixels, min/maxScrollExtent, extentBefore/After, viewportDimension, axisDirection), `context`, `depth`, `dragDetails`/`scrollDelta`, `overscroll`/`velocity`, `direction`.
**Key methods:** `dispatch(context)`; the listener returns a `bool` to stop bubbling.

### NotificationListener (scroll context)
**Type:** StatelessWidget (generic notification interceptor).
**Capabilities:** Listens for notifications of type `T` bubbling up from descendants. Parameterized as `NotificationListener<ScrollNotification>` to react to scroll events without a `ScrollController`.
**Key constructors / named constructors:** `NotificationListener<T extends Notification>({required child, onNotification})`.
**Key properties (constructor args):** `child`, `onNotification` (`bool Function(T)`; return `true` to stop bubbling).
**Key methods:** `build`.

### RefreshIndicator
**Type:** StatefulWidget (Material pull-to-refresh).
**Capabilities:** Wraps a scrollable to add pull-to-refresh: dragging past the top reveals a spinner and invokes `onRefresh`, holding it until the returned `Future` completes.
**Key constructors / named constructors:** `RefreshIndicator()`, `.adaptive()`.
**Key properties (constructor args):** `onRefresh` (required `RefreshCallback`), `child`, `displacement`, `edgeOffset`, `color`, `backgroundColor`, `strokeWidth`, `triggerMode` (`onEdge`/`anywhere`), `notificationPredicate`, `semanticsLabel`, `semanticsValue`.
**Key methods:** `createState` → `RefreshIndicatorState`; `RefreshIndicatorState.show()`.

### DraggableScrollableSheet
**Type:** StatefulWidget.
**Capabilities:** A bottom sheet whose height the user can drag between min and max fractions of the parent, with an inner scrollable that takes over once fully expanded. Driven by a `DraggableScrollableController`.
**Key constructors / named constructors:** `DraggableScrollableSheet({required builder, ...})`.
**Key properties (constructor args):** `builder` (`(context, scrollController)`; attach the controller to the inner scrollable), `initialChildSize`, `minChildSize`, `maxChildSize`, `expand`, `snap`, `snapSizes`, `snapAnimationDuration`, `controller`, `shouldCloseOnMinExtent`.
**Key methods:** `createState`. Related: `DraggableScrollableActuator.reset(context)`; `DraggableScrollableNotification`.

### ReorderableListView
**Type:** StatefulWidget (Material list with drag-to-reorder).
**Capabilities:** A list whose items can be reordered by long-press-drag, invoking `onReorder` with old/new indices. Every child must carry a unique `Key`.
**Key constructors / named constructors:** `ReorderableListView()`, `.builder()`.
**Key properties (constructor args):** `onReorder` (required; `newIndex` computed before removal), `children`/(`itemBuilder` + `itemCount`), `onReorderStart`, `onReorderEnd`, `buildDefaultDragHandles`, `header`, `footer`, `padding`, `scrollDirection`, `reverse`, `scrollController`, `physics`, `shrinkWrap`, `proxyDecorator`, `itemExtent`, `prototypeItem`, `autoScrollerVelocityScalar`, `clipBehavior`, `dragBoundaryProvider`.
**Key methods:** `createState`.

### ListWheelScrollView
**Type:** StatefulWidget.
**Capabilities:** Lays out fixed-height children on a simulated 3D cylinder (a spinning "wheel"), as in date/time pickers.
**Key constructors / named constructors:** `ListWheelScrollView()`, `.useDelegate()` (e.g. `ListWheelChildBuilderDelegate`, `ListWheelChildLoopingListDelegate`).
**Key properties (constructor args):** `itemExtent` (required), `children`/`childDelegate`, `controller` (`FixedExtentScrollController`: `selectedItem`), `physics` (usually `FixedExtentScrollPhysics`), `diameterRatio`, `perspective`, `offAxisFraction`, `useMagnifier`, `magnification`, `overAndUnderCenterOpacity`, `squeeze`, `renderChildrenOutsideViewport`, `clipBehavior`, `onSelectedItemChanged`.
**Key methods:** `createState`.

### SliverList
**Type:** Sliver widget (produces `RenderSliverList`).
**Capabilities:** A sliver that lazily lays out a linear, variable-extent list of children. The sliver equivalent of a `ListView` body.
**Key constructors / named constructors:** `SliverList({required delegate})`, `.builder({required itemBuilder, itemCount, ...})`, `.separated({required itemBuilder, required separatorBuilder, itemCount, ...})`, `.list({required children, ...})`.
**Key properties (constructor args):** `delegate` (`SliverChildListDelegate`/`SliverChildBuilderDelegate`), `itemBuilder`, `itemCount`, `separatorBuilder`, `findChildIndexCallback`, `addAutomaticKeepAlives`, `addRepaintBoundaries`, `addSemanticIndexes`.
**Key methods:** `createRenderObject` → `RenderSliverList`.

### SliverFixedExtentList
**Type:** Sliver widget (produces `RenderSliverFixedExtentList`).
**Capabilities:** Like `SliverList` but every child has the same fixed main-axis extent, allowing far more efficient layout.
**Key constructors / named constructors:** `SliverFixedExtentList({required delegate, required itemExtent})`, `.builder(...)`, `.list(...)`.
**Key properties (constructor args):** `itemExtent` (required), `delegate`/(`itemBuilder`/`itemCount`).
**Key methods:** `createRenderObject` → `RenderSliverFixedExtentList`.

### SliverPrototypeExtentList
**Type:** Sliver widget (produces `RenderSliverPrototypeExtentList`).
**Capabilities:** Uniform main-axis extent derived by measuring a single `prototypeItem` — fixed-extent performance without hardcoding the extent.
**Key constructors / named constructors:** `SliverPrototypeExtentList({required delegate, required prototypeItem})`, `.builder(...)`, `.list(...)`.
**Key properties (constructor args):** `prototypeItem` (required), `delegate`/(`itemBuilder`/`itemCount`).
**Key methods:** `createRenderObject` → `RenderSliverPrototypeExtentList`.

### SliverGrid
**Type:** Sliver widget (produces `RenderSliverGrid`).
**Capabilities:** A sliver placing children in a 2D arrangement governed by a `SliverGridDelegate`; the sliver equivalent of a `GridView` body.
**Key constructors / named constructors:** `SliverGrid({required delegate, required gridDelegate})`, `.count(...)`, `.extent(...)`, `.builder(...)`.
**Key properties (constructor args):** `gridDelegate`, `delegate`/(`itemBuilder`/`itemCount`), `crossAxisCount`/`maxCrossAxisExtent`, `mainAxisSpacing`, `crossAxisSpacing`, `childAspectRatio`.
**Key methods:** `createRenderObject` → `RenderSliverGrid`.

### SliverAppBar
**Type:** StatefulWidget (a Material app bar designed to live in a `CustomScrollView`).
**Capabilities:** An app bar integrated as a sliver, able to expand, collapse, float, pin, and stretch as the scroll offset changes. Often used with a `FlexibleSpaceBar`.
**Key constructors / named constructors:** `SliverAppBar()`, `.medium()`, `.large()`.
**Key properties (constructor args):** `pinned`, `floating`, `snap`, `expandedHeight`, `collapsedHeight`, `flexibleSpace`, `bottom`, `stretch`, `stretchTriggerOffset`, `onStretchTrigger`, `leading`, `title`, `actions`, `backgroundColor`, `elevation`, `forceElevated`, `automaticallyImplyLeading`, `primary`, `toolbarHeight`, `titleSpacing`.
**Key methods:** `createState` (drives a `SliverPersistentHeader` internally).

### SliverToBoxAdapter
**Type:** Sliver widget (produces `RenderSliverToBoxAdapter`).
**Capabilities:** Adapts a single ordinary (box) widget for placement among slivers. Use sparingly for single items; for many box children prefer a `SliverList`.
**Key constructors / named constructors:** `SliverToBoxAdapter({child})`.
**Key properties (constructor args):** `child`.
**Key methods:** `createRenderObject` → `RenderSliverToBoxAdapter`.

### SliverPadding
**Type:** Sliver widget (produces `RenderSliverPadding`).
**Capabilities:** Applies padding around another sliver in both axes.
**Key constructors / named constructors:** `SliverPadding({required padding, sliver})`.
**Key properties (constructor args):** `padding`, `sliver`.
**Key methods:** `createRenderObject` → `RenderSliverPadding`.

### SliverFillRemaining
**Type:** Sliver widget.
**Capabilities:** A sliver whose single child fills the remaining viewport space after preceding slivers (e.g. push a footer down or fill a short list).
**Key constructors / named constructors:** `SliverFillRemaining({child, hasScrollBody, fillOverscroll})`.
**Key properties (constructor args):** `child`, `hasScrollBody` (default true), `fillOverscroll`.
**Key methods:** `createRenderObject`.

### SliverFillViewport
**Type:** Sliver widget (produces `RenderSliverFillViewport`).
**Capabilities:** Lays out each child to fill a fraction (`viewportFraction`) of the viewport main-axis extent — the sliver behind `PageView`-like full-screen paging within a `CustomScrollView`.
**Key constructors / named constructors:** `SliverFillViewport({required delegate, viewportFraction, padEnds})`.
**Key properties (constructor args):** `delegate`, `viewportFraction` (default 1.0), `padEnds`.
**Key methods:** `createRenderObject` → `RenderSliverFillViewport`.

### SliverPersistentHeader
**Type:** Sliver widget.
**Capabilities:** A sliver header that can stay pinned and/or float, with extent shrinking/growing between `minExtent` and `maxExtent`. Layout/appearance supplied by a `SliverPersistentHeaderDelegate`.
**Key constructors / named constructors:** `SliverPersistentHeader({required delegate, pinned, floating})`.
**Key properties (constructor args):** `delegate`, `pinned`, `floating`.
**Key methods:** `createRenderObject`.
**SliverPersistentHeaderDelegate (overridable):** `build(BuildContext, double shrinkOffset, bool overlapsContent)` → `Widget`; `minExtent`; `maxExtent`; `shouldRebuild(...)` → `bool`; optional `vsync`, `snapConfiguration`, `stretchConfiguration`, `showOnScreenConfiguration`.

### SliverSafeArea
**Type:** Sliver widget.
**Capabilities:** The sliver equivalent of `SafeArea`: insets a child sliver by ambient `MediaQuery` padding on chosen sides.
**Key constructors / named constructors:** `SliverSafeArea({sliver, left, top, right, bottom, minimum})`.
**Key properties (constructor args):** `sliver`, `left`/`top`/`right`/`bottom` (default `true`), `minimum`.
**Key methods:** `build` (wraps in a `SliverPadding`).

### SliverAnimatedList
**Type:** StatefulWidget (sliver with animated insert/remove).
**Capabilities:** The sliver analogue of `AnimatedList`; managed via `SliverAnimatedListState` (or `GlobalKey<SliverAnimatedListState>`).
**Key constructors / named constructors:** `SliverAnimatedList({required itemBuilder, initialItemCount, ...})`.
**Key properties (constructor args):** `itemBuilder` (`(context, index, animation)`), `initialItemCount`, `findChildIndexCallback`.
**Key methods:** `createState` → `SliverAnimatedListState` (`insertItem`, `insertAllItems`, `removeItem`, `removeAllItems`); static `of`/`maybeOf`.

### SliverOpacity
**Type:** Sliver widget (produces `RenderSliverOpacity`).
**Capabilities:** The sliver equivalent of `Opacity`. Non-opaque values force a costly offscreen layer.
**Key constructors / named constructors:** `SliverOpacity({required opacity, sliver, alwaysIncludeSemantics})`.
**Key properties (constructor args):** `opacity`, `sliver`, `alwaysIncludeSemantics`.
**Key methods:** `createRenderObject` → `RenderSliverOpacity`.

### SliverFadeTransition
**Type:** Sliver widget (animated).
**Capabilities:** The sliver equivalent of `FadeTransition`: animates a child sliver's opacity from an `Animation<double>`.
**Key constructors / named constructors:** `SliverFadeTransition({required opacity, sliver, alwaysIncludeSemantics})`.
**Key properties (constructor args):** `opacity` (`Animation<double>`), `sliver`, `alwaysIncludeSemantics`.
**Key methods:** `build`/`createRenderObject`.

### SliverVisibility
**Type:** Sliver widget (StatelessWidget composing sliver primitives).
**Capabilities:** The sliver equivalent of `Visibility`: conditionally shows/hides a child sliver, with options to keep it laid out, painted, interactive, or in semantics when hidden.
**Key constructors / named constructors:** `SliverVisibility({required sliver, replacementSliver, visible, ...})`, `.maintain({required sliver, visible, ...})`.
**Key properties (constructor args):** `sliver`, `replacementSliver`, `visible`, `maintainState`, `maintainAnimation`, `maintainSize`, `maintainSemantics`, `maintainInteractivity`.
**Key methods:** `build`.

### SliverIgnorePointer
**Type:** Sliver widget (produces `RenderSliverIgnorePointer`).
**Capabilities:** The sliver equivalent of `IgnorePointer`: makes a child sliver invisible to hit testing while still visible.
**Key constructors / named constructors:** `SliverIgnorePointer({sliver, ignoring, ignoringSemantics})`.
**Key properties (constructor args):** `sliver`, `ignoring`, `ignoringSemantics` (deprecated in recent SDKs).
**Key methods:** `createRenderObject` → `RenderSliverIgnorePointer`.

### ScrollController (supporting)
**Type:** Listenable controller (`ChangeNotifier`).
**Capabilities:** Creates, owns, and exposes the `ScrollPosition`(s) of attached scrollables; read the offset, set `initialScrollOffset`, and programmatically animate/jump.
**Key constructors / named constructors:** `ScrollController({initialScrollOffset, keepScrollOffset, debugLabel, onAttach, onDetach})`.
**Key properties:** `offset`, `position`/`positions`, `hasClients`, `initialScrollOffset`.
**Key methods:** `animateTo(offset, {duration, curve})`, `jumpTo(offset)`, `attach`/`detach`, `createScrollPosition`, `dispose`.

### ScrollPhysics subclasses (supporting)
**Type:** `ScrollPhysics` (immutable description of scroll response/simulations).
**Capabilities:** Determine how a scrollable responds to input and momentum; composed via `applyTo`/`parent`.
- **BouncingScrollPhysics** — iOS-style overscroll that stretches and bounces back.
- **ClampingScrollPhysics** — Android-style clamp at edges (pairs with an overscroll glow).
- **AlwaysScrollableScrollPhysics** — always allow scrolling/overscroll even when content fits (useful with `RefreshIndicator`).
- **NeverScrollableScrollPhysics** — disable user scrolling (still scrollable via a controller).
- Also `RangeMaintainingScrollPhysics`, `FixedExtentScrollPhysics`, `PageScrollPhysics`.
**Key methods (overridable):** `applyPhysicsToUserOffset`, `applyBoundaryConditions`, `createBallisticSimulation`, `adjustPositionForNewDimensions`, `minFlingVelocity`/`maxFlingVelocity`, `shouldAcceptUserOffset`, `applyTo`.

---

## Painting, effects, images & icons

### Image
**Type:** StatefulWidget (renders a `dart:ui` image via `RawImage`).
**Capabilities:** Displays an image from various sources (network, asset, file, memory), handling decoding, scaling, caching, and frame/loading callbacks. Supports placeholder/error builders and animated images (GIF/WebP).
**Key constructors / named constructors:**
- `Image({required ImageProvider image, ...})` — generic.
- `Image.network(String src, {double scale, Map<String,String>? headers, ...})`.
- `Image.asset(String name, {AssetBundle? bundle, String? package, double? scale, ...})` — resolution-aware via asset variants.
- `Image.file(File file, {double scale, ...})` — not supported on web.
- `Image.memory(Uint8List bytes, {double scale, ...})`.
**Key properties (constructor args):**
- `image`, `width`, `height`, `fit` (`BoxFit`), `alignment`, `repeat` (`ImageRepeat`), `color` + `colorBlendMode`, `colorFilter`, `loadingBuilder`, `frameBuilder`, `errorBuilder`, `gaplessPlayback`, `filterQuality`, `semanticLabel`, `excludeFromSemantics`, `centerSlice` (nine-patch), `isAntiAlias`, `matchTextDirection`.
**Key methods:** `createState()` (`_ImageState`); rendering flows through a `RawImage`. (`precacheImage()` is a top-level helper.)

### Icon
**Type:** StatelessWidget.
**Capabilities:** Draws a glyph from an icon font (e.g. Material Icons) as a graphical, non-interactive symbol. Honors `IconTheme` and supports text-direction mirroring.
**Key constructors / named constructors:** `Icon(IconData? icon, {...})`.
**Key properties (constructor args):**
- `icon` (`IconData?`), `size` (falls back to `IconTheme.size`, default 24), `color` (falls back to `IconTheme.color`), `fill`, `weight`, `grade`, `opticalSize` (variable-font axes), `shadows`, `semanticLabel`, `textDirection`.
**Key methods:** `build()`.

### ImageIcon
**Type:** StatelessWidget.
**Capabilities:** Renders an `ImageProvider` as if it were an icon — sized and tinted by the ambient `IconTheme` — when a true icon font isn't available.
**Key constructors / named constructors:** `ImageIcon(ImageProvider? image, {double? size, Color? color, String? semanticLabel})`.
**Key properties (constructor args):** `image`, `size`, `color` (tint via `BlendMode.srcIn`), `semanticLabel`.
**Key methods:** `build()`.

### IconButton (cross-ref)
**Type:** StatelessWidget. *(Full detail in the buttons section.)*
**Capabilities:** An interactive, tappable `Icon` with Material ink response, padding, tooltip, and pressed/disabled states. `onPressed: null` disables it.

### FadeInImage
**Type:** StatefulWidget.
**Capabilities:** Shows a placeholder image first, then cross-fades to a target image once loaded, avoiding a blank/jarring pop-in.
**Key constructors / named constructors:** `FadeInImage({required placeholder, required image, ...})`, `.assetNetwork({required String placeholder, required String image, ...})`, `.memoryNetwork({required Uint8List placeholder, required String image, ...})`.
**Key properties (constructor args):** `placeholder`, `image`, `placeholderErrorBuilder`, `imageErrorBuilder`, `fadeInDuration`, `fadeOutDuration`, `fadeInCurve`, `fadeOutCurve`, `width`, `height`, `fit`, `placeholderFit`, `alignment`, `repeat`, `matchTextDirection`, `excludeFromSemantics`, `imageSemanticLabel`.
**Key methods:** `createState()`.

### CircleAvatar (cross-ref)
**Type:** StatelessWidget. *(Full detail in the Material display section.)*
**Capabilities:** A circle representing a user — backed by `backgroundImage`, `backgroundColor`, or `child` (initials). Key props: `radius`/`minRadius`/`maxRadius`, `foregroundImage`, `onBackgroundImageError`.

### RawImage
**Type:** Leaf render-object widget (`RenderImage`).
**Capabilities:** The low-level primitive that paints an already-decoded `dart:ui.Image` (what `Image` builds on). You manage the decoded image lifecycle yourself.
**Key constructors / named constructors:** `RawImage({ui.Image? image, ...})`.
**Key properties (constructor args):** `image`, `width`, `height`, `scale`, `fit`, `alignment`, `repeat`, `centerSlice`, `color`, `colorBlendMode`, `opacity` (`Animation<double>?`), `filterQuality`, `matchTextDirection`, `textDirection`, `invertColors`, `isAntiAlias`, `debugImageLabel`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderImage`.

### AssetBundle (usage)
**Type:** Abstract class (not a widget).
**Capabilities:** Provides access to bundled assets (strings, byte data, structured resources). The ambient bundle is reached via `DefaultAssetBundle.of(context)` (falling back to `rootBundle`).
**Key constructors / named constructors:** Concrete subclasses: `rootBundle`, `NetworkAssetBundle`, `PlatformAssetBundle`.
**Key methods:** `loadString(key)`, `loadStructuredData(key, parser)`, `load(key)` → `ByteData`, `evict(key)`.

### Opacity
**Type:** Single-child render-object widget (`RenderOpacity`).
**Capabilities:** Makes its child partially/fully transparent by compositing into an intermediate layer. Comparatively expensive for values strictly between 0 and 1.
**Key constructors / named constructors:** `Opacity({required double opacity, Widget? child, ...})`.
**Key properties (constructor args):** `opacity` (0.0–1.0), `alwaysIncludeSemantics`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderOpacity`.

### AnimatedOpacity (cross-ref)
**Type:** Implicitly-animated StatefulWidget. *(Full detail in the animation section.)*
**Capabilities:** Animates `opacity` over `duration`/`curve` whenever the target changes. For explicit control use `FadeTransition`.

### Transform
**Type:** Single-child render-object widget (`RenderTransform`).
**Capabilities:** Applies a 4×4 matrix to its child before painting, optionally affecting origin and hit-testing. A pure paint-time transform (does not change layout space).
**Key constructors / named constructors:** `Transform({required Matrix4 transform, Offset? origin, AlignmentGeometry? alignment, bool transformHitTests, FilterQuality? filterQuality, ...})`, `.rotate({required angle, ...})`, `.scale({scale/scaleX/scaleY, ...})`, `.translate({required Offset offset, ...})`, `.flip({flipX, flipY, ...})`.
**Key properties (constructor args):** `transform`, `origin`, `alignment`, `transformHitTests`, `filterQuality`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderTransform`.

### ClipRect
**Type:** Single-child render-object widget (`RenderClipRect`).
**Capabilities:** Clips its child to a rectangle (by default its own bounds). Commonly used with `CustomClipper<Rect>` or before effects like `BackdropFilter`.
**Key constructors / named constructors:** `ClipRect({CustomClipper<Rect>? clipper, Clip clipBehavior = Clip.hardEdge, Widget? child})`.
**Key properties (constructor args):** `clipper`, `clipBehavior` (`none`/`hardEdge`/`antiAlias`/`antiAliasWithSaveLayer`), `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderClipRect`.

### ClipRRect
**Type:** Single-child render-object widget (`RenderClipRRect`).
**Capabilities:** Clips its child to a rounded rectangle defined by `borderRadius`.
**Key constructors / named constructors:** `ClipRRect({BorderRadiusGeometry? borderRadius = BorderRadius.zero, CustomClipper<RRect>? clipper, Clip clipBehavior = Clip.antiAlias, Widget? child})`.
**Key properties (constructor args):** `borderRadius`, `clipper`, `clipBehavior` (default `antiAlias`), `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderClipRRect`.

### ClipOval
**Type:** Single-child render-object widget (`RenderClipOval`).
**Capabilities:** Clips its child to an axis-aligned oval/ellipse inscribed in the clip rect (a circle when square). Used for circular avatars/thumbnails.
**Key constructors / named constructors:** `ClipOval({CustomClipper<Rect>? clipper, Clip clipBehavior = Clip.antiAlias, Widget? child})`.
**Key properties (constructor args):** `clipper`, `clipBehavior`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderClipOval`.

### ClipPath
**Type:** Single-child render-object widget (`RenderClipPath`).
**Capabilities:** Clips its child to an arbitrary `Path` from a `CustomClipper<Path>`. Enables non-rectangular shapes.
**Key constructors / named constructors:** `ClipPath({CustomClipper<Path>? clipper, Clip clipBehavior = Clip.antiAlias, Widget? child})`, `.shape({required ShapeBorder shape, Clip clipBehavior, Widget? child})`.
**Key properties (constructor args):** `clipper` (null clips to bounding rect), `clipBehavior`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderClipPath`.

### CustomClipper (delegate)
**Type:** Abstract delegate class (`CustomClipper<T>`), not a widget.
**Capabilities:** Supplies the clip geometry for the `Clip*` widgets, parameterized by `Rect`/`RRect`/`Path`. Can repaint reactively via a `reclip` `Listenable`.
**Key constructors / named constructors:** `CustomClipper({Listenable? reclip})`.
**Key methods:** `getClip(Size size)` → `T`; `shouldReclip(covariant CustomClipper<T> oldClipper)` → `bool`; `getApproximateClipRect(Size size)` (optional culling).

### DecoratedBox
**Type:** Single-child render-object widget (`RenderDecoratedBox`).
**Capabilities:** Paints a `Decoration` (typically `BoxDecoration`/`ShapeDecoration`) behind or in front of its child — backgrounds, borders, gradients, shadows, background images.
**Key constructors / named constructors:** `DecoratedBox({required Decoration decoration, DecorationPosition position = DecorationPosition.background, Widget? child})`.
**Key properties (constructor args):** `decoration`, `position` (`background`/`foreground`), `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderDecoratedBox`.

### Container (decoration) (cross-ref)
**Type:** StatelessWidget convenience. *(Full detail in the layout section.)*
**Capabilities:** Composes padding, margin, constraints, `decoration`/`foregroundDecoration`, and `transform`. Its `decoration` (a `BoxDecoration`) is the everyday way to get backgrounds, borders, radii, and shadows. Setting both `color` and `decoration` is an error — put `color` inside the decoration.

### BackdropFilter
**Type:** Single-child render-object widget (`RenderBackdropFilter`).
**Capabilities:** Applies an `ImageFilter` (e.g. blur) to the already-painted content *behind* the widget (frosted-glass effects). Clip it (e.g. with `ClipRect`) to bound the expensive backdrop.
**Key constructors / named constructors:** `BackdropFilter({required ImageFilter filter, BlendMode blendMode = BlendMode.srcOver, Widget? child})`.
**Key properties (constructor args):** `filter` (e.g. `ImageFilter.blur(...)`), `blendMode`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderBackdropFilter`.

### ImageFiltered
**Type:** Single-child render-object widget (`RenderImageFiltered`).
**Capabilities:** Applies an `ImageFilter` to the widget's *own* painted content (blur, dilate, matrix), unlike `BackdropFilter` which filters what's behind.
**Key constructors / named constructors:** `ImageFiltered({required ImageFilter imageFilter, bool enabled = true, Widget? child})`.
**Key properties (constructor args):** `imageFilter`, `enabled`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()`.

### ColorFiltered
**Type:** Single-child render-object widget (`RenderColorFiltered`).
**Capabilities:** Applies a `ColorFilter` (matrix, mode-based tint) to its child's painted output. Useful for tinting, grayscale, or blend-mode recoloring.
**Key constructors / named constructors:** `ColorFiltered({required ColorFilter colorFilter, Widget? child})`.
**Key properties (constructor args):** `colorFilter` (e.g. `ColorFilter.mode(...)` / `ColorFilter.matrix(...)`), `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderColorFiltered`.

### ShaderMask
**Type:** Single-child render-object widget (`RenderShaderMask`).
**Capabilities:** Applies a `Shader` (typically a gradient) as a mask over its child using a `BlendMode` — e.g. fade-out edges, gradient-tinted text.
**Key constructors / named constructors:** `ShaderMask({required ShaderCallback shaderCallback, BlendMode blendMode = BlendMode.modulate, Widget? child})`.
**Key properties (constructor args):** `shaderCallback` (`Shader Function(Rect bounds)`), `blendMode`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderShaderMask`.

### CustomPaint
**Type:** Single-child render-object widget (`RenderCustomPaint`).
**Capabilities:** Provides a `Canvas` for arbitrary 2D drawing behind (`painter`) and/or in front of (`foregroundPainter`) an optional child.
**Key constructors / named constructors:** `CustomPaint({CustomPainter? painter, CustomPainter? foregroundPainter, Size size = Size.zero, bool isComplex = false, bool willChange = false, Widget? child})`.
**Key properties (constructor args):** `painter`, `foregroundPainter`, `size` (when no child), `isComplex`, `willChange` (raster-cache hints), `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderCustomPaint`.

### CustomPainter (delegate)
**Type:** Abstract delegate class (`CustomPainter`), not a widget; extend for `CustomPaint`.
**Capabilities:** Encapsulates drawing logic and repaint/hit-test decisions. Can repaint reactively via a `repaint` `Listenable`.
**Key constructors / named constructors:** `CustomPainter({Listenable? repaint})`.
**Key methods:** `paint(Canvas canvas, Size size)`; `shouldRepaint(covariant CustomPainter oldDelegate)` → `bool`; `hitTest(Offset position)` → `bool?`; `shouldRebuildSemantics(...)` and `semanticsBuilder` (optional).

### RepaintBoundary
**Type:** Single-child render-object widget (`RenderRepaintBoundary`).
**Capabilities:** Isolates its subtree onto its own compositing layer so repaints don't propagate to/from siblings — a key performance optimization. Also captures a subtree as an image (`toImage`/`toImageSync`).
**Key constructors / named constructors:** `RepaintBoundary({Widget? child})`, `.wrap(Widget child, int childIndex)`, `.wrapAll(List<Widget>)`.
**Key properties (constructor args):** `child`.
**Key methods:** `createRenderObject()` → `RenderRepaintBoundary` (exposes `toImage()`/`toImageSync()`).

### Texture
**Type:** Leaf render-object widget (`RenderTexture` / `TextureBox`).
**Capabilities:** Displays a backend texture (a `textureId`) supplied by the engine/platform — camera previews, video players, native rendering surfaces.
**Key constructors / named constructors:** `Texture({required int textureId, bool freeze = false, FilterQuality filterQuality = FilterQuality.low})`.
**Key properties (constructor args):** `textureId`, `freeze`, `filterQuality`.
**Key methods:** `createRenderObject()`/`updateRenderObject()`.

### PhysicalModel
**Type:** Single-child render-object widget (`RenderPhysicalModel`).
**Capabilities:** Gives its child a physical "material" appearance — elevation (with shadow), background color, and a rectangle/rounded-rectangle shape that clips the child.
**Key constructors / named constructors:** `PhysicalModel({BoxShape shape = BoxShape.rectangle, Clip clipBehavior = Clip.none, BorderRadius? borderRadius, double elevation = 0.0, required Color color, Color shadowColor = const Color(0xFF000000), Widget? child})`.
**Key properties (constructor args):** `shape`, `borderRadius`, `elevation`, `color`, `shadowColor`, `clipBehavior`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderPhysicalModel`.

### PhysicalShape
**Type:** Single-child render-object widget (`RenderPhysicalShape`).
**Capabilities:** Like `PhysicalModel` but clips/shapes the child to an arbitrary path via a `CustomClipper<Path>`, casting an elevation shadow that follows the shape.
**Key constructors / named constructors:** `PhysicalShape({required CustomClipper<Path> clipper, Clip clipBehavior = Clip.none, double elevation = 0.0, required Color color, Color shadowColor = const Color(0xFF000000), Widget? child})`.
**Key properties (constructor args):** `clipper`, `elevation`, `color`, `shadowColor`, `clipBehavior`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderPhysicalShape`.

### Card (cross-ref)
**Type:** StatelessWidget. *(Full detail in the Material section.)*
**Capabilities:** A rounded, elevated Material panel for grouping content. Key props: `elevation`, `color`/`surfaceTintColor`, `shape`, `margin`, `clipBehavior`; `Card.filled`/`Card.outlined`.

### Material (elevation) (cross-ref)
**Type:** StatefulWidget. *(Full detail in the Material foundations section.)*
**Capabilities:** The piece of Material on which ink effects and elevation shadows render. `elevation`, `color`, `shadowColor`, `surfaceTintColor`, `shape`, `clipBehavior` define the surface; provides the canvas for `InkWell`/`InkResponse` splashes.

### DecoratedBoxTransition (cross-ref)
**Type:** Animated transition widget. *(Full detail in the animation section.)*
**Capabilities:** Animates a `DecoratedBox`'s `decoration` from an `Animation<Decoration>` (typically a `DecorationTween`).

### Banner
**Type:** Single-child render-object widget (`RenderBanner`).
**Capabilities:** Paints a diagonal ribbon with a short message across a corner of its child — the mechanism behind the debug "DEBUG" banner (`CheckedModeBanner`).
**Key constructors / named constructors:** `Banner({required String message, required BannerLocation location, Color color, TextStyle textStyle, TextDirection? textDirection, TextDirection? layoutDirection, ...})`.
**Key properties (constructor args):** `message`, `location`, `color`, `textStyle`, `textDirection`, `layoutDirection`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderBanner`.

### Visibility
**Type:** StatelessWidget.
**Capabilities:** Conditionally shows/hides its `child`. When hidden it can keep space (via a `replacement`) or collapse, and can independently maintain state, animation, size, semantics, and interactivity.
**Key constructors / named constructors:** `Visibility({required Widget child, Widget replacement = const SizedBox.shrink(), bool visible = true, bool maintainState, bool maintainAnimation, bool maintainSize, bool maintainSemantics, bool maintainInteractivity, ...})`, `.maintain({...})`.
**Key properties (constructor args):** `visible`, `replacement`, `maintainState`, `maintainAnimation`, `maintainSize`, `maintainSemantics`, `maintainInteractivity`, `child`.
**Key methods:** `build()` (composes `Offstage`/`SizedBox`/`IgnorePointer`/`ExcludeSemantics`).

### Offstage (cross-ref)
**Type:** Single-child render-object widget (`RenderOffstage`). *(Full detail in the layout section.)*
**Capabilities:** When `offstage` is true, lays the child out (measurable) but does not paint/hit-test it or take space. Key prop: `offstage`.

### IgnorePointer
**Type:** Single-child render-object widget (`RenderIgnorePointer`).
**Capabilities:** Makes its subtree invisible to hit-testing (events pass through to widgets behind) while still painting normally.
**Key constructors / named constructors:** `IgnorePointer({bool ignoring = true, bool? ignoringSemantics (deprecated), Widget? child})`.
**Key properties (constructor args):** `ignoring`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderIgnorePointer`.

### AbsorbPointer
**Type:** Single-child render-object widget (`RenderAbsorbPointer`).
**Capabilities:** Like `IgnorePointer` but *absorbs* pointer events itself — the subtree won't receive them and they do **not** pass through to widgets behind.
**Key constructors / named constructors:** `AbsorbPointer({bool absorbing = true, Widget? child})`.
**Key properties (constructor args):** `absorbing`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderAbsorbPointer`.

### MouseRegion
**Type:** Single-child render-object widget (`RenderMouseRegion`).
**Capabilities:** Detects mouse enter/exit/hover over its region and sets the cursor — the basis for hover effects on desktop/web. Does not respond to touch/click.
**Key constructors / named constructors:** `MouseRegion({PointerEnterEventListener? onEnter, PointerExitEventListener? onExit, PointerHoverEventListener? onHover, MouseCursor cursor = MouseCursor.defer, bool opaque = true, HitTestBehavior? hitTestBehavior, Widget? child})`.
**Key properties (constructor args):** `onEnter`, `onHover`, `onExit`, `cursor` (e.g. `SystemMouseCursors.click`), `opaque`, `hitTestBehavior`, `child`.
**Key methods:** `createRenderObject()`/`updateRenderObject()` → `RenderMouseRegion`.

### Tooltip (cross-ref)
**Type:** StatefulWidget. *(Full detail in the Material interaction section.)*
**Capabilities:** Shows an explanatory text label on long-press (touch) or hover (mouse/desktop). Key props: `message`/`richMessage`, `waitDuration`, `showDuration`, `preferBelow`, `triggerMode`, `decoration`, `textStyle`.

---

## Animation & motion

### AnimatedContainer
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** A `Container` that automatically animates between values when its properties change. Animates layout, decoration, and transform changes.
**Key constructors / named constructors:** `AnimatedContainer({...})`.
**Key properties (constructor args):** `duration` (required), `curve` (default `Curves.linear`), `alignment`, `padding`, `margin`, `color`/`decoration`/`foregroundDecoration`, `width`, `height`, `constraints`, `transform`, `transformAlignment`, `clipBehavior`, `onEnd`, `child`.
**Key methods:** `createState()` (logic inherited from `ImplicitlyAnimatedWidget`/`AnimatedWidgetBaseState`).

### AnimatedPadding
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates changes to its `padding`.
**Key constructors / named constructors:** `AnimatedPadding({...})`.
**Key properties (constructor args):** `padding` (required), `duration` (required), `curve`, `onEnd`, `child`.
**Key methods:** `createState()`.

### AnimatedAlign
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates changes to its `alignment` (and optionally `widthFactor`/`heightFactor`).
**Key constructors / named constructors:** `AnimatedAlign({...})`.
**Key properties (constructor args):** `alignment` (required), `widthFactor`, `heightFactor`, `duration` (required), `curve`, `onEnd`, `child`.
**Key methods:** `createState()`.

### AnimatedPositioned
**Type:** Implicitly animated StatefulWidget (only inside a `Stack`).
**Capabilities:** Animates a child's position (and size) within a `Stack`.
**Key constructors / named constructors:** `AnimatedPositioned({...})`, `.fromRect({required Rect rect, ...})`.
**Key properties (constructor args):** `left`, `top`, `right`, `bottom`, `width`, `height`, `duration` (required), `curve`, `onEnd`, `child` (required).
**Key methods:** `createState()`.

### AnimatedPositionedDirectional
**Type:** Implicitly animated StatefulWidget (inside a `Stack`).
**Capabilities:** Directional (RTL/LTR-aware) variant of `AnimatedPositioned`, using `start`/`end`.
**Key constructors / named constructors:** `AnimatedPositionedDirectional({...})`.
**Key properties (constructor args):** `start`, `top`, `end`, `bottom`, `width`, `height`, `duration` (required), `curve`, `onEnd`, `child` (required).
**Key methods:** `createState()`.

### AnimatedOpacity
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates the opacity of its child. Wraps an `Opacity` whose value is tweened.
**Key constructors / named constructors:** `AnimatedOpacity({...})`.
**Key properties (constructor args):** `opacity` (required, [0,1]), `alwaysIncludeSemantics`, `duration` (required), `curve`, `onEnd`, `child`.
**Key methods:** `createState()`.

### AnimatedDefaultTextStyle
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates changes to the default `TextStyle` for descendant `Text` widgets (interpolating lerp-able fields).
**Key constructors / named constructors:** `AnimatedDefaultTextStyle({...})`.
**Key properties (constructor args):** `style` (required), `textAlign`, `softWrap`, `overflow`, `maxLines`, `textWidthBasis`, `textHeightBehavior`, `child` (required), `duration` (required), `curve`, `onEnd`.
**Key methods:** `createState()`.

### AnimatedPhysicalModel
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates the elevation (and optionally color/shadow color) of a physical-model surface.
**Key constructors / named constructors:** `AnimatedPhysicalModel({...})`.
**Key properties (constructor args):** `elevation` (required), `color` (required) + `animateColor`, `shadowColor` (required) + `animateShadowColor`, `shape` (`BoxShape`), `borderRadius`, `clipBehavior`, `child` (required), `duration` (required), `curve`, `onEnd`.
**Key methods:** `createState()`.

### AnimatedSize
**Type:** Implicitly animated StatefulWidget (requires a `TickerProvider`).
**Capabilities:** Automatically animates its own size to fit its child whenever the child's size changes.
**Key constructors / named constructors:** `AnimatedSize({...})`.
**Key properties (constructor args):** `duration` (required), `reverseDuration`, `curve`, `alignment` (default `center`), `clipBehavior` (default `hardEdge`), `child`.
**Key methods:** `createState()`.

### AnimatedScale
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates the scale of its child (uniform factor) via an underlying `Transform.scale`.
**Key constructors / named constructors:** `AnimatedScale({...})`.
**Key properties (constructor args):** `scale` (required), `alignment` (default `center`), `filterQuality`, `child`, `duration` (required), `curve`, `onEnd`.
**Key methods:** `createState()`.

### AnimatedRotation
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates rotation in `turns` (1.0 = 360°) via an underlying `Transform.rotate`.
**Key constructors / named constructors:** `AnimatedRotation({...})`.
**Key properties (constructor args):** `turns` (required), `alignment` (default `center`), `filterQuality`, `child`, `duration` (required), `curve`, `onEnd`.
**Key methods:** `createState()`.

### AnimatedSlide
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates the position of its child by an `Offset` expressed as a fraction of the child's size.
**Key constructors / named constructors:** `AnimatedSlide({...})`.
**Key properties (constructor args):** `offset` (required), `child`, `duration` (required), `curve`, `onEnd`.
**Key methods:** `createState()`.

### AnimatedFractionallySizedBox
**Type:** Implicitly animated StatefulWidget.
**Capabilities:** Animates the `widthFactor`/`heightFactor` and `alignment` of a `FractionallySizedBox`.
**Key constructors / named constructors:** `AnimatedFractionallySizedBox({...})`.
**Key properties (constructor args):** `widthFactor`, `heightFactor`, `alignment` (default `center`), `child`, `duration` (required), `curve`, `onEnd`.
**Key methods:** `createState()`.

### TweenAnimationBuilder
**Type:** Implicitly animated builder widget (StatefulWidget, generic `<T>`).
**Capabilities:** Drives an animation toward a target `Tween.end` whenever the tween changes, rebuilding via a `builder` with the current value; one-off implicit animations without an explicit controller.
**Key constructors / named constructors:** `TweenAnimationBuilder<T>({...})`.
**Key properties (constructor args):** `tween` (required; `end` is the target), `builder` (required `ValueWidgetBuilder<T>`), `duration` (required), `curve`, `onEnd`, `child`.
**Key methods:** `createState()`.

### AnimatedBuilder
**Type:** StatefulWidget (subclass of `AnimatedWidget`).
**Capabilities:** Rebuilds via a `builder` whenever the supplied `Listenable` (commonly an `Animation`) notifies; drive arbitrary widgets from a controller without subclassing `AnimatedWidget`.
**Key constructors / named constructors:** `AnimatedBuilder({required Listenable animation, required TransitionBuilder builder, Widget? child})`.
**Key properties (constructor args):** `animation`, `builder` (`(context, child)`), `child` (built once, passed back).
**Key methods:** `build(context)` (from `AnimatedWidget`, delegates to `builder`).

### AnimatedWidget
**Type:** Abstract base widget (StatefulWidget) for explicit transitions.
**Capabilities:** Rebuilds itself whenever the `Listenable` it is given changes; subclasses (e.g. `FadeTransition`) override `build` to render using the current animation value.
**Key constructors / named constructors:** `AnimatedWidget({required Listenable listenable})`.
**Key properties (constructor args):** `listenable`.
**Key methods:** `build(BuildContext)` — abstract; `createState()` → `_AnimatedState` (manages add/remove listener and `setState`).

### FadeTransition
**Type:** Explicit transition widget.
**Capabilities:** Animates the opacity of its child from a driving `Animation<double>`.
**Key constructors / named constructors:** `FadeTransition({required Animation<double> opacity, ...})`.
**Key properties (constructor args):** `opacity` (required), `alwaysIncludeSemantics`, `child`.
**Key methods:** `build(context)`.

### ScaleTransition
**Type:** Explicit transition widget.
**Capabilities:** Scales its child according to a driving `Animation<double>`.
**Key constructors / named constructors:** `ScaleTransition({required Animation<double> scale, ...})`.
**Key properties (constructor args):** `scale` (required), `alignment` (default `center`), `filterQuality`, `child`.
**Key methods:** `build(context)`.

### RotationTransition
**Type:** Explicit transition widget.
**Capabilities:** Rotates its child by an `Animation<double>` in turns (1.0 = 360°).
**Key constructors / named constructors:** `RotationTransition({required Animation<double> turns, ...})`.
**Key properties (constructor args):** `turns` (required), `alignment` (default `center`), `filterQuality`, `child`.
**Key methods:** `build(context)`.

### SlideTransition
**Type:** Explicit transition widget.
**Capabilities:** Translates its child by an `Animation<Offset>` expressed as a fraction of the child's size.
**Key constructors / named constructors:** `SlideTransition({required Animation<Offset> position, ...})`.
**Key properties (constructor args):** `position` (required), `transformHitTests` (default true), `textDirection`, `child`.
**Key methods:** `build(context)`.

### SizeTransition
**Type:** Explicit transition widget.
**Capabilities:** Animates its own size along one axis by clipping/revealing the child according to an `Animation<double>` factor.
**Key constructors / named constructors:** `SizeTransition({required Animation<double> sizeFactor, ...})`.
**Key properties (constructor args):** `sizeFactor` (required), `axis` (default `vertical`), `axisAlignment` (-1.0 to 1.0), `fixedCrossAxisSizeFactor`, `child`.
**Key methods:** `build(context)`.

### PositionedTransition
**Type:** Explicit transition widget (inside a `Stack`).
**Capabilities:** Animates a child's absolute position/size in a `Stack` from an `Animation<RelativeRect>`.
**Key constructors / named constructors:** `PositionedTransition({required Animation<RelativeRect> rect, required Widget child})`.
**Key properties (constructor args):** `rect` (required), `child` (required).
**Key methods:** `build(context)`.

### RelativePositionedTransition
**Type:** Explicit transition widget (inside a `Stack`).
**Capabilities:** Animates a child's position within a `Stack` by interpolating a `Rect` relative to a known `size`.
**Key constructors / named constructors:** `RelativePositionedTransition({required Animation<Rect?> rect, required Size size, required Widget child})`.
**Key properties (constructor args):** `rect` (required), `size` (required), `child` (required).
**Key methods:** `build(context)`.

### DecoratedBoxTransition
**Type:** Explicit transition widget.
**Capabilities:** Animates a `Decoration` (color, border, gradient, boxShadow) from an `Animation<Decoration>` (typically a `DecorationTween`).
**Key constructors / named constructors:** `DecoratedBoxTransition({required Animation<Decoration> decoration, ...})`.
**Key properties (constructor args):** `decoration` (required), `position` (default `background`), `child` (required).
**Key methods:** `build(context)`.

### AlignTransition
**Type:** Explicit transition widget.
**Capabilities:** Animates the `alignment` of its child from an `Animation<AlignmentGeometry>`.
**Key constructors / named constructors:** `AlignTransition({required Animation<AlignmentGeometry> alignment, required Widget child, ...})`.
**Key properties (constructor args):** `alignment` (required), `widthFactor`, `heightFactor`, `child` (required).
**Key methods:** `build(context)`.

### DefaultTextStyleTransition
**Type:** Explicit transition widget.
**Capabilities:** Animates the default `TextStyle` applied to descendant `Text` widgets from an `Animation<TextStyle>` (typically a `TextStyleTween`).
**Key constructors / named constructors:** `DefaultTextStyleTransition({required Animation<TextStyle> style, required Widget child, ...})`.
**Key properties (constructor args):** `style` (required), `textAlign`, `softWrap`, `overflow`, `maxLines`, `textWidthBasis`, `textHeightBehavior`, `child` (required).
**Key methods:** `build(context)`.

### AnimatedModalBarrier
**Type:** Explicit transition widget.
**Capabilities:** A `ModalBarrier` whose `color` is driven by an `Animation<Color>`; fades the scrim behind modal routes/dialogs.
**Key constructors / named constructors:** `AnimatedModalBarrier({required Animation<Color?> color, ...})`.
**Key properties (constructor args):** `color` (required), `dismissible` (default true), `onDismiss`, `semanticsLabel`, `barrierSemanticsDismissible`.
**Key methods:** `build(context)`.

### AnimatedSwitcher
**Type:** Switcher widget (StatefulWidget).
**Capabilities:** Cross-fades (or otherwise transitions) between its children when the `child` is swapped for a new one with a different key.
**Key constructors / named constructors:** `AnimatedSwitcher({...})`.
**Key properties (constructor args):** `child` (swap requires a differing `Key`), `duration` (required), `reverseDuration`, `switchInCurve`, `switchOutCurve`, `transitionBuilder` (default fades), `layoutBuilder` (default stacks centered).
**Key methods:** `createState()`; `defaultTransitionBuilder`/`defaultLayoutBuilder` (static).

### AnimatedCrossFade
**Type:** Switcher widget (StatefulWidget).
**Capabilities:** Cross-fades between exactly two children based on a `crossFadeState`, simultaneously animating size.
**Key constructors / named constructors:** `AnimatedCrossFade({...})`.
**Key properties (constructor args):** `firstChild`, `secondChild` (required), `crossFadeState` (required), `duration` (required), `reverseDuration`, `firstCurve`, `secondCurve`, `sizeCurve`, `alignment` (default `topCenter`), `layoutBuilder`.
**Key methods:** `createState()`; `defaultLayoutBuilder` (static).

### AnimatedList
**Type:** Animated scrolling list widget (StatefulWidget).
**Capabilities:** A `ListView`-like widget that animates items as they are inserted/removed via its state object.
**Key constructors / named constructors:** `AnimatedList({...})`, `.separated({...})`.
**Key properties (constructor args):** `itemBuilder` (required `(context, index, animation)`), `initialItemCount` (required), `key` (often `GlobalKey<AnimatedListState>`), `scrollDirection`, `controller`, `physics`, `shrinkWrap`, `padding`, `reverse`, `clipBehavior`.
**Key methods:** `createState()`; static `AnimatedList.of(context)`/`maybeOf`.

### AnimatedListState
**Type:** State object for `AnimatedList` (`State<AnimatedList>` with `TickerProviderStateMixin`).
**Capabilities:** Imperatively drives item insertion/removal animations and tracks the current count. Obtained via a `GlobalKey<AnimatedListState>` or `AnimatedList.of`.
**Key methods:** `insertItem(int index, {Duration duration})`; `insertAllItems(int index, int length, {Duration duration})`; `removeItem(int index, AnimatedRemovedItemBuilder builder, {Duration duration})`; `removeAllItems(...)`.

### SliverAnimatedList
**Type:** Sliver variant of `AnimatedList` (StatefulWidget, sliver).
**Capabilities:** Same insert/remove animation semantics as `AnimatedList` but as a sliver for use inside a `CustomScrollView`; driven via `SliverAnimatedListState`.
**Key constructors / named constructors:** `SliverAnimatedList({...})`.
**Key properties (constructor args):** `itemBuilder` (required), `initialItemCount` (required), `findChildIndexCallback`.
**Key methods:** `createState()`; static `of`/`maybeOf`; state exposes `insertItem`/`removeItem`.

### Dismissible
**Type:** Gesture-driven dismissal widget (StatefulWidget).
**Capabilities:** Lets the user swipe a child away (e.g. list items), animating it off-screen and reporting the dismissal.
**Key constructors / named constructors:** `Dismissible({required Key key, required Widget child, ...})`.
**Key properties (constructor args):** `key` (required), `child` (required), `background`, `secondaryBackground`, `onDismissed` (`DismissDirectionCallback`), `confirmDismiss`, `direction` (default `horizontal`), `dismissThresholds`, `movementDuration`, `resizeDuration`, `crossAxisEndOffset`, `behavior`, `onUpdate`, `onResize`.
**Key methods:** `createState()`.

### Hero
**Type:** Shared-element transition widget (StatefulWidget).
**Capabilities:** Animates a widget ("hero") flying between two routes during a navigation push/pop, matching heroes by `tag`.
**Key constructors / named constructors:** `Hero({required Object tag, required Widget child, ...})`.
**Key properties (constructor args):** `tag` (required), `child` (required), `createRectTween`, `flightShuttleBuilder`, `placeholderBuilder`, `transitionOnUserGestures`.
**Key methods:** `createState()`.

### AnimatedIcon
**Type:** Animated icon widget (StatefulWidget).
**Capabilities:** Renders an icon that morphs between two states (e.g. play/pause) driven by an `Animation<double>` using a predefined `AnimatedIconData`.
**Key constructors / named constructors:** `AnimatedIcon({required AnimatedIconData icon, required Animation<double> progress, ...})`.
**Key properties (constructor args):** `icon` (required), `progress` (required, 0.0–1.0), `color`, `size`, `semanticLabel`, `textDirection`.
**Key methods:** `createState()`.

### AnimatedIcons (supporting)
**Type:** Supporting collection class (not a widget).
**Capabilities:** A namespace of predefined `AnimatedIconData` constants describing two-state morphing icons for `AnimatedIcon`.
**Key properties:** Static members such as `menu_arrow`, `menu_close`, `play_pause`, `arrow_menu`, `add_event`, `ellipsis_search`, `home_menu`, `list_view`, `view_list`.
**Key methods:** N/A.

### AnimationController (supporting)
**Type:** Supporting controller class — an `Animation<double>` you drive (not a widget).
**Capabilities:** Generates a stream of values (default 0.0–1.0) over a duration using a `Ticker`; can run forward, in reverse, repeat, fling with physics, and notifies listeners each frame.
**Key constructors / named constructors:** `AnimationController({required TickerProvider vsync, Duration? duration, ...})`, `.unbounded({...})`.
**Key properties (constructor args):** `vsync` (required), `duration`, `reverseDuration`, `lowerBound`, `upperBound`, `value`, `status`, `animationBehavior`.
**Key methods:** `forward({double? from})`, `reverse({double? from})`, `repeat({...})`, `animateTo`/`animateBack`, `fling`, `stop`, `reset`, `dispose()` (required), `addListener`/`addStatusListener`.

### Animation (supporting)
**Type:** Supporting abstract class (`Animation<T>`); not a widget.
**Capabilities:** Represents a value of type `T` that changes over time plus a status, notifying listeners; produced by controllers and tweens, consumed by transition widgets.
**Key properties:** `value`, `status` (`AnimationStatus`), `isCompleted`, `isDismissed`, `isAnimating`.
**Key methods:** `addListener`/`removeListener`, `addStatusListener`/`removeStatusListener`, `drive(Animatable<U>)`.

### Tween (supporting)
**Type:** Supporting class (`Tween<T>`, subclass of `Animatable<T>`); not a widget.
**Capabilities:** Defines an interpolation between `begin` and `end`, mapping an animation's 0.0–1.0 into a value of type `T` via `lerp`.
**Key constructors / named constructors:** `Tween<T>({T? begin, T? end})`; specialized subclasses: `ColorTween`, `SizeTween`, `RectTween`, `IntTween`, `DecorationTween`, `TextStyleTween`, `AlignmentTween`, `BorderRadiusTween`, `CurveTween`.
**Key properties (constructor args):** `begin`, `end`.
**Key methods:** `lerp(double t)`, `transform(double t)`, `animate(Animation<double> parent)`, `chain(Animatable<double>)`.

### CurvedAnimation (supporting)
**Type:** Supporting class (`Animation<double>`); not a widget.
**Capabilities:** Wraps a parent `Animation<double>` and applies a `Curve` (and optional reverse curve), producing non-linear easing without altering the controller.
**Key constructors / named constructors:** `CurvedAnimation({required Animation<double> parent, required Curve curve, Curve? reverseCurve})`.
**Key properties (constructor args):** `parent` (required), `curve` (required), `reverseCurve`.
**Key methods:** `dispose()`; inherits `Animation` listener methods.

### Curves (supporting)
**Type:** Supporting collection class (not a widget); a namespace of `Curve` constants.
**Capabilities:** A catalog of predefined easing curves used by implicit (`curve:`) and explicit (`CurvedAnimation`) animations.
**Key properties:** `linear`, `easeIn`, `easeOut`, `easeInOut`, `fastOutSlowIn`, `bounceIn`, `bounceOut`, `elasticIn`, `elasticOut`, `easeInOutCubic`, `decelerate`.
**Key methods:** Each constant exposes `transform(double t)` and `flipped`.

### TickerProvider / SingleTickerProviderStateMixin / TickerProviderStateMixin (supporting)
**Type:** Supporting abstraction + State mixins (not widgets).
**Capabilities:** `TickerProvider` vends `Ticker`s (frame callbacks) to drive `AnimationController`s; the mixins implement it on a `State`, tying ticker lifetime to the widget and muting tickers when the route is not visible.
**Key methods:** `createTicker(TickerCallback onTick)` → `Ticker`. `SingleTickerProviderStateMixin` provides one ticker (single controller); `TickerProviderStateMixin` provides multiple. Both clean up on `dispose` and are used as the `vsync` argument.

---

## Gestures, interaction, async & accessibility

### GestureDetector
**Type:** StatelessWidget (non-visual gesture recognition wrapper).
**Capabilities:** Detects discrete and continuous pointer gestures (taps, double-taps, long presses, drags/pans, scales) on its child and surfaces them as high-level callbacks. Sizes from its child (or expands to fill if `child` is null and constraints are bounded).
**Key constructors / named constructors:** `GestureDetector({child, onTap, onDoubleTap, onLongPress, onPanUpdate, onScaleUpdate, ..., behavior, excludeFromSemantics, dragStartBehavior})`.
**Key properties (constructor args):**
- `onTap`, `onTapDown`, `onTapUp`, `onTapCancel`; `onDoubleTap`.
- `onLongPress`, `onLongPressStart`, `onLongPressMoveUpdate`, `onLongPressEnd`.
- `onVerticalDragUpdate`, `onHorizontalDragUpdate`, `onPanStart`/`onPanUpdate`/`onPanEnd` (mutually constrained to avoid arena conflicts).
- `onScaleStart`/`onScaleUpdate`/`onScaleEnd` (`ScaleUpdateDetails`: scale, rotation, focalPoint).
- `behavior` (`HitTestBehavior`: `deferToChild`/`opaque`/`translucent`), `excludeFromSemantics`, `dragStartBehavior`.
**Key methods:** `build` (creates a `RawGestureDetector` configured with `GestureRecognizer` factories).

### InkWell (cross-ref)
**Type:** StatefulWidget. *(Full detail in the Material section.)*
**Capabilities:** A rectangular area that responds to touches with Material ink splash and highlight; like `GestureDetector` but with feedback and requiring a `Material` ancestor. Use `InkResponse` for non-rectangular splashes.
**Key properties (constructor args):** `onTap`, `onDoubleTap`, `onLongPress`, `onHover`, `onFocusChange`, `splashColor`, `highlightColor`, `hoverColor`, `focusColor`, `overlayColor`, `borderRadius`, `customBorder`, `child`.
**Key methods:** `build`, `createState`.

### Listener
**Type:** Single-child render-object widget (raw pointer listener).
**Capabilities:** Calls callbacks on raw pointer events (down/move/up/cancel/hover/scroll) without gesture-arena disambiguation. Lower-level than `GestureDetector`.
**Key constructors / named constructors:** `Listener({onPointerDown, onPointerMove, onPointerUp, onPointerHover, onPointerSignal, onPointerCancel, behavior, child})`.
**Key properties (constructor args):** `onPointerDown`, `onPointerMove`, `onPointerUp`, `onPointerCancel`, `onPointerHover`, `onPointerSignal` (e.g. `PointerScrollEvent`), `behavior` (`HitTestBehavior`).
**Key methods:** `createRenderObject`/`updateRenderObject` (`RenderPointerListener`).

### MouseRegion
**Type:** Single-child render-object widget (mouse tracking).
**Capabilities:** Tracks the mouse entering, moving within, and exiting a region, and can set the hover cursor. Used for hover states and custom cursors on desktop/web.
**Key constructors / named constructors:** `MouseRegion({onEnter, onExit, onHover, cursor, opaque, hitTestBehavior, child})`.
**Key properties (constructor args):** `onEnter`, `onExit`, `onHover`, `cursor` (e.g. `SystemMouseCursors.click`), `opaque`.
**Key methods:** `createRenderObject`/`updateRenderObject` (`RenderMouseRegion`).

### RawGestureDetector
**Type:** StatefulWidget (low-level gesture host).
**Capabilities:** The engine behind `GestureDetector`; supply your own map of `GestureRecognizer` factories for full control over recognition and arena participation.
**Key constructors / named constructors:** `RawGestureDetector({gestures, behavior, excludeFromSemantics, semantics, child})`.
**Key properties (constructor args):** `gestures` (`Map<Type, GestureRecognizerFactory>`), `behavior`, `excludeFromSemantics`, `semantics` (`SemanticsGestureDelegate`).
**Key methods:** `createState` → `RawGestureDetectorState` (`replaceGestureRecognizers`, `replaceSemanticsActions`).

### Draggable
**Type:** StatefulWidget (drag source). Generic: `Draggable<T>`.
**Capabilities:** Makes its child draggable; shows a `feedback` widget under the pointer and carries a `data` payload that compatible `DragTarget<T>`s can accept on drop.
**Key constructors / named constructors:** `Draggable({child, feedback, data, childWhenDragging, axis, affinity, maxSimultaneousDrags, ...})`.
**Key properties (constructor args):** `data`, `feedback`, `child`, `childWhenDragging`, `axis`, `feedbackOffset`, `dragAnchorStrategy`, `maxSimultaneousDrags`, `onDragStarted`, `onDragUpdate`, `onDragEnd`, `onDragCompleted`, `onDraggableCanceled`.
**Key methods:** `createState`, `build`.

### LongPressDraggable
**Type:** StatefulWidget (subclass of `Draggable<T>`).
**Capabilities:** Same as `Draggable` but the drag is initiated by a long press, useful inside scrollables where an immediate drag would conflict with scrolling.
**Key constructors / named constructors:** `LongPressDraggable({data, feedback, child, childWhenDragging, delay, hapticFeedbackOnStart, ...})`.
**Key properties (constructor args):** Inherits `Draggable` properties; `delay`, `hapticFeedbackOnStart`.
**Key methods:** `createState` (uses a `DelayedMultiDragGestureRecognizer`).

### DragTarget
**Type:** StatefulWidget (drop target). Generic: `DragTarget<T>`.
**Capabilities:** Receives data dragged from `Draggable<T>` widgets, rebuilding its appearance as candidates hover and invoking acceptance callbacks on drop.
**Key constructors / named constructors:** `DragTarget({builder, onWillAcceptWithDetails, onAcceptWithDetails, onLeave, onMove, hitTestBehavior})`.
**Key properties (constructor args):** `builder` (`(context, candidateData, rejectedData)`), `onWillAcceptWithDetails`, `onAcceptWithDetails`, `onMove`, `onLeave`, `hitTestBehavior`.
**Key methods:** `createState`, `build`.

### DraggableScrollableSheet (cross-ref)
**Type:** StatefulWidget. *(Full detail in the scrolling section.)*
**Capabilities:** A bottom sheet whose extent the user can drag between min/max fractions; its builder receives a `ScrollController` so the inner scrollable and the drag work together.
**Key properties (constructor args):** `builder` (`(context, scrollController)`), `initialChildSize`, `minChildSize`, `maxChildSize`, `snap`, `snapSizes`, `expand`, `controller`.
**Key methods:** `createState`.

### Dismissible (cross-ref)
**Type:** StatefulWidget. *(Full detail in the animation section.)*
**Capabilities:** Lets the user dismiss its child by swiping; commonly swipe-to-delete list items. Requires a unique `key`.
**Key properties (constructor args):** `key` (required), `child`, `onDismissed`, `confirmDismiss`, `background`, `secondaryBackground`, `direction`, `resizeDuration`, `dismissThresholds`.
**Key methods:** `createState`, `build`.

### InteractiveViewer
**Type:** StatefulWidget (pan/zoom container).
**Capabilities:** Lets the user pan, zoom (pinch/scroll-wheel), and optionally drag its child within a viewport, with configurable scale limits and boundaries; useful for images, maps, diagrams.
**Key constructors / named constructors:** `InteractiveViewer({child, minScale, maxScale, panEnabled, scaleEnabled, boundaryMargin, transformationController, constrained, ...})`, `.builder({builder, ...})` (lazily builds visible content for large/infinite content).
**Key properties (constructor args):** `minScale`, `maxScale`, `panEnabled`, `scaleEnabled`, `boundaryMargin`, `constrained`, `transformationController` (`TransformationController`, a `Matrix4`), `onInteractionStart`/`onInteractionUpdate`/`onInteractionEnd`.
**Key methods:** `createState`; `transformationController.value` (a `Matrix4`) can be read/set.

### ReorderableListView (cross-ref)
**Type:** StatefulWidget. *(Full detail in the scrolling section.)*
**Capabilities:** A list whose items the user can drag to reorder; each child needs a unique key, and `onReorder` reports old/new indices.
**Key properties (constructor args):** `onReorder` (account for index shift when `newIndex > oldIndex`), `children`/(`itemBuilder` + `itemCount`), `header`, `footer`, `padding`, `scrollDirection`, `buildDefaultDragHandles`, `proxyDecorator`.
**Key methods:** `createState`. (`ReorderableDragStartListener`/`ReorderableDelayedDragStartListener` for custom handles.)

### GestureRecognizer
**Type:** Abstract base class (gesture recognition), not a widget. *(Supporting.)*
**Capabilities:** The objects that recognize gestures from raw pointer events and compete in the gesture arena. `GestureDetector`/`RawGestureDetector` instantiate concrete subclasses.
**Notable subclasses:** `TapGestureRecognizer`, `DoubleTapGestureRecognizer`, `LongPressGestureRecognizer`, `PanGestureRecognizer`, `ScaleGestureRecognizer`, `HorizontalDragGestureRecognizer`/`VerticalDragGestureRecognizer`, `MultiTapGestureRecognizer`, `DelayedMultiDragGestureRecognizer`.
**Key methods:** `addPointer(PointerDownEvent)`, `dispose()`, arena hooks `acceptGesture`/`rejectGesture`.

### Focus (cross-ref)
**Type:** StatefulWidget. *(Full detail in the input/focus section.)*
**Capabilities:** Manages a `FocusNode` for its subtree, enabling keyboard focus, traversal, and key-event handling.
**Key properties (constructor args):** `focusNode`, `autofocus`, `onFocusChange`, `onKeyEvent`, `canRequestFocus`, `skipTraversal`.
**Key methods:** `createState`; `Focus.of(context)`.

### Shortcuts / Actions (cross-ref)
**Type:** Inherited/stateful widgets. *(Full detail in the input/focus section.)*
**Capabilities:** `Shortcuts` maps `LogicalKeySet`/`ShortcutActivator` to `Intent`s; `Actions` maps `Intent` types to `Action`s. Together they decouple key bindings from logic.
**Key properties (constructor args):** `Shortcuts.shortcuts`, `Actions.actions`.
**Key methods:** `Actions.invoke(context, intent)` / `Actions.maybeInvoke(...)`.

### FocusTraversalGroup (cross-ref)
**Type:** StatefulWidget. *(Full detail in the input/focus section.)*
**Capabilities:** Groups descendant focusables so Tab/arrow traversal stays within and orders the group by a policy.
**Key properties (constructor args):** `policy`, `descendantsAreFocusable`, `descendantsAreTraversable`, `child`.
**Key methods:** `createState`.

### FutureBuilder
**Type:** StatefulWidget (async snapshot builder). Generic: `FutureBuilder<T>`.
**Capabilities:** Subscribes to a `Future<T>` and rebuilds its `builder` with an `AsyncSnapshot<T>` reflecting waiting/data/error states.
**Key constructors / named constructors:** `FutureBuilder({future, initialData, builder})`.
**Key properties (constructor args):** `future` (identity matters; recreating it on every build re-triggers), `initialData`, `builder` (`(context, AsyncSnapshot<T>)`).
**Key methods:** `createState`, `build`. Inspect `snapshot.connectionState` (`none`/`waiting`/`active`/`done`), `snapshot.hasData`/`snapshot.data`, `snapshot.hasError`/`snapshot.error`.

### StreamBuilder
**Type:** StatefulWidget (async snapshot builder over a stream). Generic: `StreamBuilder<T>`.
**Capabilities:** Subscribes to a `Stream<T>` and rebuilds with an `AsyncSnapshot<T>` on each event/error/state change.
**Key constructors / named constructors:** `StreamBuilder({stream, initialData, builder})`.
**Key properties (constructor args):** `stream`, `initialData`, `builder`.
**Key methods:** `build`; overrides `StreamBuilderBase`'s `initial`, `afterData`, `afterError`, `afterDone`, `afterConnected`/`afterDisconnected`. Read `snapshot.connectionState`, `snapshot.data`, `snapshot.error`.

### StreamBuilderBase
**Type:** Abstract StatefulWidget (stream-folding base). *(Mention.)*
**Capabilities:** Generic base (`StreamBuilderBase<T, S>`) managing subscription lifecycle and folding events into an arbitrary summary type `S`; `StreamBuilder` summarizes into `AsyncSnapshot<T>`.
**Key constructors / named constructors:** `StreamBuilderBase({stream})`.
**Key methods (to override):** `initial()`, `afterConnected(S)`, `afterData(S, T)`, `afterError(S, Object, StackTrace)`, `afterDone(S)`, `afterDisconnected(S)`, `build(context, S)`.

### ValueListenableBuilder (cross-ref)
**Type:** StatefulWidget. Generic: `ValueListenableBuilder<T>`. *(Full detail in the core section.)*
**Capabilities:** Rebuilds whenever a `ValueListenable<T>` emits a new value, exposing the current value to its builder; supports a `child` optimization.
**Key properties (constructor args):** `valueListenable`, `builder` (`(context, T value, Widget? child)`), `child`.
**Key methods:** `createState`, `build`.

### ListenableBuilder
**Type:** StatefulWidget (generic Listenable listener).
**Capabilities:** Rebuilds whenever a `Listenable` notifies (`ChangeNotifier`, `Animation`, `AnimationController`); the general form behind `AnimatedBuilder`/`ValueListenableBuilder` when you need "rebuild on notify" rather than a value.
**Key constructors / named constructors:** `ListenableBuilder({listenable, builder, child})`.
**Key properties (constructor args):** `listenable`, `builder` (`(context, Widget? child)`; read state from the listenable), `child`.
**Key methods:** `createState`, `build`.

### Semantics
**Type:** Single-child render-object widget (semantics annotation).
**Capabilities:** Annotates its subtree with accessibility semantics (labels, roles, states, actions) consumed by screen readers and assistive technologies. The primary tool for making custom widgets accessible.
**Key constructors / named constructors:** `Semantics({child, label, hint, value, properties..., onTap, ...})`; `Semantics.fromProperties({properties, child})`.
**Key properties (constructor args):**
- `label`, `hint`, `value`, `increasedValue`/`decreasedValue`.
- `button`, `header`, `image`, `textField`, `checked`, `selected`, `enabled`, `focusable`, `focused`, `liveRegion`.
- `onTap`, `onLongPress`, `onIncrease`, `onDecrease`, `onScrollUp`/`onScrollDown`.
- `container`, `excludeSemantics`, `sortKey` (`SemanticsSortKey`).
**Key methods:** `createRenderObject`/`updateRenderObject` (`RenderSemanticsAnnotations`).

### MergeSemantics
**Type:** Single-child render-object widget.
**Capabilities:** Merges the semantics of its descendants into a single node, so a composite widget (e.g. a labeled checkbox row) is announced as one unit.
**Key constructors / named constructors:** `MergeSemantics({child})`.
**Key properties (constructor args):** `child`.
**Key methods:** `createRenderObject`/`updateRenderObject` (`RenderMergeSemantics`).

### ExcludeSemantics
**Type:** Single-child render-object widget.
**Capabilities:** Removes its subtree's semantics entirely, hiding decorative content from assistive technologies.
**Key constructors / named constructors:** `ExcludeSemantics({excluding = true, child})`.
**Key properties (constructor args):** `excluding`, `child`.
**Key methods:** `createRenderObject`/`updateRenderObject` (`RenderExcludeSemantics`).

### BlockSemantics
**Type:** Single-child render-object widget.
**Capabilities:** Drops the semantics of widgets painted *before* it within the same container, so an overlay (e.g. a modal) hides the content beneath from assistive technologies.
**Key constructors / named constructors:** `BlockSemantics({blocking = true, child})`.
**Key properties (constructor args):** `blocking`, `child`.
**Key methods:** `createRenderObject`/`updateRenderObject` (`RenderBlockSemantics`).

### IndexedSemantics
**Type:** Single-child render-object widget.
**Capabilities:** Assigns an explicit index to a semantics node, used by scrollables/lists so AT can announce item position correctly (e.g. "item 3 of 10") even with lazy building.
**Key constructors / named constructors:** `IndexedSemantics({index, child})`.
**Key properties (constructor args):** `index`, `child`.
**Key methods:** `createRenderObject`/`updateRenderObject` (`RenderIndexedSemantics`).

### Semantics properties overview
**Type:** Supporting concept (`SemanticsProperties` + the semantics model).
**Capabilities:** `SemanticsProperties` bundles every semantic attribute settable via `Semantics.fromProperties` — descriptive text, role/state flags, and action callbacks — which the framework compiles into the platform accessibility tree.
**Key groupings:**
- Descriptive: `label`, `value`, `increasedValue`, `decreasedValue`, `hint`, `tooltip`, `attributedLabel`/`attributedValue`.
- Roles/states: `enabled`, `checked`, `mixed`, `selected`, `toggled`, `button`, `link`, `header`, `textField`, `readOnly`, `obscured`, `multiline`, `image`, `liveRegion`, `hidden`, `focusable`, `focused`.
- Actions: `onTap`, `onLongPress`, `onScrollLeft`/`Right`/`Up`/`Down`, `onIncrease`, `onDecrease`, `onCopy`, `onCut`, `onPaste`, `onDismiss`, `onSetSelection`, `onDidGainAccessibilityFocus`/`onDidLoseAccessibilityFocus`.
- Traversal/grouping: `sortKey`, `container`, `explicitChildNodes`, `namesRoute`, `scopesRoute`.
**Key methods:** Consumed via `RenderObject.describeSemanticsConfiguration` (a `SemanticsConfiguration`); surfaced as `SemanticsNode`s; tunable globally via `SemanticsBinding`/`MediaQuery.accessibleNavigation`.

### ExcludeFocus
**Type:** StatefulWidget (focus suppression).
**Capabilities:** Removes its subtree from focus traversal and prevents descendants gaining focus — the focus-tree analogue of `ExcludeSemantics`.
**Key constructors / named constructors:** `ExcludeFocus({excluding = true, child})`.
**Key properties (constructor args):** `excluding`, `child`.
**Key methods:** `createState`, `build` (wraps a `Focus` with `canRequestFocus`/`descendantsAreFocusable` disabled).

### SelectionContainer
**Type:** StatefulWidget (selection scoping). *(Brief.)*
**Capabilities:** Scopes and customizes text selection for a subtree under a `SelectableRegion`; `SelectionContainer.disabled` opts a subtree out of selection.
**Key constructors / named constructors:** `SelectionContainer({registrar, delegate, child})`, `.disabled({child})`.
**Key properties (constructor args):** `delegate` (`SelectionContainerDelegate`), `registrar` (`SelectionRegistrar`), `child`.
**Key methods:** `createState`.

### Navigator
**Type:** StatefulWidget (route stack manager).
**Capabilities:** Manages a stack of `Route` objects and the transitions between them, providing imperative (`push`/`pop`) and declarative (`pages`) navigation. `MaterialApp`/`WidgetsApp` insert a root `Navigator` automatically.
**Key constructors / named constructors:** `Navigator({pages, onPopPage / onDidRemovePage, initialRoute, onGenerateRoute, onUnknownRoute, observers, key})`.
**Key properties (constructor args):** `pages` (declarative 2.0 API, paired with `onDidRemovePage`/`onPopPage`), `initialRoute`, `onGenerateRoute`, `onUnknownRoute`, `observers` (`NavigatorObserver`s).
**Key methods (static):**
- `Navigator.of(context)` → `NavigatorState` (use `rootNavigator: true` for root).
- `Navigator.push(context, route)` / `pushReplacement` / `pushAndRemoveUntil`.
- `Navigator.pop(context, [result])` / `maybePop` / `popUntil`.
- `Navigator.pushNamed(context, name, {arguments})` / `pushReplacementNamed` / `popAndPushNamed` / `restorablePushNamed`.
- `Navigator.canPop(context)`.

### Router
**Type:** StatefulWidget (declarative routing). *(Brief.)*
**Capabilities:** Drives a `Navigator` from platform route information (URLs/deep links), the foundation of Router (Navigator 2.0) and packages like `go_router`.
**Key constructors / named constructors:** `Router({routerDelegate, routeInformationParser, routeInformationProvider, backButtonDispatcher})`, `.withConfig({config})`.
**Key properties (constructor args):** `routerDelegate`, `routeInformationParser`, `routeInformationProvider`, `backButtonDispatcher`.
**Key methods:** `Router.of(context)`; `Router.navigate` / `Router.neglect`.

### MaterialPageRoute
**Type:** Modal route (`PageRoute<T>` subclass). *(Supporting.)*
**Capabilities:** A full-screen route using platform-appropriate Material transitions; the route type most commonly pushed for whole-page navigation.
**Key constructors / named constructors:** `MaterialPageRoute({builder, settings, maintainState, fullscreenDialog, allowSnapshotting})`.
**Key properties (constructor args):** `builder` (`(context)`), `settings` (`RouteSettings`), `maintainState`, `fullscreenDialog`.
**Key methods:** Inherits `Route`/`ModalRoute` lifecycle (`buildPage`, `buildTransitions`, `didPush`, `didPop`).

### PageRouteBuilder
**Type:** Modal route (`PageRoute<T>` subclass). *(Supporting.)*
**Capabilities:** Builds a route with fully custom page content and transition animations via callbacks.
**Key constructors / named constructors:** `PageRouteBuilder({pageBuilder, transitionsBuilder, transitionDuration, reverseTransitionDuration, opaque, barrierColor, barrierDismissible, settings})`.
**Key properties (constructor args):** `pageBuilder` (`(context, animation, secondaryAnimation)`), `transitionsBuilder` (`(context, animation, secondaryAnimation, child)`), `transitionDuration`/`reverseTransitionDuration`, `opaque`, `barrierColor`, `barrierDismissible`.
**Key methods:** Inherits `ModalRoute` lifecycle.

### Hero (cross-ref)
**Type:** StatefulWidget (shared-element transition). *(Full detail in the animation section.)*
**Capabilities:** Animates a shared widget flying between its positions on two routes; matching `Hero`s are paired by `tag`.
**Key properties (constructor args):** `tag`, `child`, `flightShuttleBuilder`, `placeholderBuilder`, `transitionOnUserGestures`.
**Key methods:** `createState`, `build`. (The enclosing `Navigator` runs a `HeroController`.)

### WillPopScope
**Type:** StatefulWidget (back-navigation interception). **Deprecated — use `PopScope`.**
**Capabilities:** Intercepts back-button/pop attempts via an async callback returning whether the route may pop. Deprecated in favor of `PopScope`.
**Key constructors / named constructors:** `WillPopScope({onWillPop, child})`.
**Key properties (constructor args):** `onWillPop` (`Future<bool> Function()`; false blocks the pop), `child`.
**Key methods:** `createState`.

### PopScope
**Type:** StatefulWidget (back-navigation control). Generic: `PopScope<T>`.
**Capabilities:** The modern replacement for `WillPopScope`: controls whether the enclosing route can pop and reacts after a pop attempt, compatible with Android predictive back.
**Key constructors / named constructors:** `PopScope({canPop, onPopInvokedWithResult, child})`.
**Key properties (constructor args):** `canPop` (synchronous bool), `onPopInvokedWithResult` (`(bool didPop, T? result)`, replaces deprecated `onPopInvoked`), `child`.
**Key methods:** `createState`, `build`.

### ModalRoute
**Type:** Abstract route class (`TransitionRoute<T>` with a modal barrier). *(Mention.)*
**Capabilities:** Base class for routes that obscure and block interaction with routes beneath them via a modal barrier; `PageRoute` and dialog/popup routes derive from it.
**Key properties / members:** `barrierColor`, `barrierDismissible`, `barrierLabel`, `maintainState`, `opaque`.
**Key methods:** `ModalRoute.of(context)` (read the current route, `RouteSettings`, animations); `buildPage`, `buildTransitions`, `addLocalHistoryEntry`, `isCurrent`/`canPop`.

### RouteObserver
**Type:** `NavigatorObserver` subclass. *(Mention.)* Generic: `RouteObserver<R extends Route>`.
**Capabilities:** Notifies subscribed `RouteAware` objects when their route is pushed, popped, or covered/uncovered — useful for pausing/resuming work based on route visibility.
**Key constructors / named constructors:** `RouteObserver()`.
**Key methods:** `subscribe(RouteAware, Route)`, `unsubscribe(RouteAware)`; overrides `didPush`/`didPop`/`didReplace`/`didRemove` and forwards `didPopNext`/`didPushNext`/`didPush`/`didPop` to subscribers.

---

## Cupertino (iOS-style)

### CupertinoApp
**Type:** StatefulWidget (application root; wraps `WidgetsApp` with Cupertino styling).
**Capabilities:** Sets up an iOS-style application with Cupertino theming, navigation, and localization defaults. The Cupertino counterpart to `MaterialApp`.
**Key constructors / named constructors:** `CupertinoApp(...)`; `CupertinoApp.router(...)`.
**Key properties (constructor args):** `home`, `theme` (`CupertinoThemeData`), `routes`, `initialRoute`, `onGenerateRoute`, `onUnknownRoute`, `navigatorKey`, `navigatorObservers`, `routerDelegate`, `routeInformationParser`, `routerConfig`, `backButtonDispatcher`, `title`, `onGenerateTitle`, `color`, `locale`, `localizationsDelegates`, `supportedLocales`, `showPerformanceOverlay`, `debugShowCheckedModeBanner`, `builder`, `scrollBehavior`.
**Key methods:** `createState()`.

### CupertinoPageScaffold
**Type:** StatefulWidget.
**Capabilities:** Basic iOS-style page layout with a content area plus an optional fixed or translucent navigation bar; manages background color and safe-area insets.
**Key constructors / named constructors:** `CupertinoPageScaffold({navigationBar, backgroundColor, resizeToAvoidBottomInset = true, required child})`.
**Key properties (constructor args):** `child`, `navigationBar` (an `ObstructingPreferredSizeWidget`), `backgroundColor`, `resizeToAvoidBottomInset`.
**Key methods:** `createState()`.

### CupertinoTabScaffold
**Type:** StatefulWidget.
**Capabilities:** An iOS tabbed layout: a `CupertinoTabBar` at the bottom and a content area built per selected tab, preserving each tab's state.
**Key constructors / named constructors:** `CupertinoTabScaffold({required tabBar, required tabBuilder, controller, backgroundColor, resizeToAvoidBottomInset = true, restorationId})`.
**Key properties (constructor args):** `tabBar`, `tabBuilder` (`IndexedWidgetBuilder`), `controller` (`CupertinoTabController`), `backgroundColor`, `resizeToAvoidBottomInset`, `restorationId`.
**Key methods:** `createState()`.

### CupertinoTabBar
**Type:** StatelessWidget (implements `PreferredSizeWidget`).
**Capabilities:** Bottom tab bar with iOS styling: a row of icon/label items, active/inactive coloring, and an optional translucent background.
**Key constructors / named constructors:** `CupertinoTabBar({required items, currentIndex = 0, onTap, backgroundColor, activeColor, inactiveColor, iconSize = 30.0, height = 50.0, border})`.
**Key properties (constructor args):** `items` (`BottomNavigationBarItem`s, ≥ 2), `currentIndex`, `onTap`, `activeColor`, `inactiveColor`, `backgroundColor`, `iconSize`, `height`, `border`.
**Key methods:** `build(context)`; `copyWith(...)`; `preferredSize`.

### CupertinoTabView
**Type:** StatefulWidget.
**Capabilities:** A root content pane for a single tab that owns its own `Navigator`, so each tab maintains an independent navigation stack.
**Key constructors / named constructors:** `CupertinoTabView({builder, navigatorKey, defaultTitle, routes, onGenerateRoute, onUnknownRoute, navigatorObservers, restorationScopeId})`.
**Key properties (constructor args):** `builder`, `routes`, `onGenerateRoute`, `onUnknownRoute`, `navigatorKey`, `navigatorObservers`, `defaultTitle`, `restorationScopeId`.
**Key methods:** `createState()`.

### CupertinoNavigationBar
**Type:** StatefulWidget (implements `ObstructingPreferredSizeWidget`).
**Capabilities:** Fixed-height iOS top navigation bar with leading/middle/trailing slots, an automatic back button, and an optionally translucent background.
**Key constructors / named constructors:** `CupertinoNavigationBar({leading, automaticallyImplyLeading = true, automaticallyImplyMiddle = true, previousPageTitle, middle, trailing, border, backgroundColor, brightness, padding, transitionBetweenRoutes = true, heroTag})`.
**Key properties (constructor args):** `leading`, `middle`, `trailing`, `automaticallyImplyLeading`, `previousPageTitle`, `backgroundColor`, `border`, `brightness`, `padding`, `transitionBetweenRoutes`, `heroTag`.
**Key methods:** `createState()`; `shouldFullyObstruct(context)`; `preferredSize`.

### CupertinoSliverNavigationBar
**Type:** StatefulWidget (a sliver; used in `CustomScrollView`).
**Capabilities:** iOS large-title navigation bar that scrolls: a large title collapses into a standard centered middle title as the user scrolls.
**Key constructors / named constructors:** `CupertinoSliverNavigationBar({largeTitle, leading, automaticallyImplyLeading = true, automaticallyImplyTitle = true, previousPageTitle, middle, trailing, border, backgroundColor, brightness, padding, transitionBetweenRoutes = true, heroTag, stretch = false})`.
**Key properties (constructor args):** `largeTitle`, `middle`, `leading`, `trailing`, `previousPageTitle`, `automaticallyImplyLeading`, `automaticallyImplyTitle`, `backgroundColor`, `border`, `brightness`, `padding`, `stretch`, `transitionBetweenRoutes`, `heroTag`.
**Key methods:** `createState()`.

### CupertinoButton
**Type:** StatefulWidget.
**Capabilities:** iOS-style button that fades its opacity on tap; plain (text) or filled, with rounded corners and a disabled state.
**Key constructors / named constructors:** `CupertinoButton({required child, onPressed, ...})`; `CupertinoButton.filled({...})`.
**Key properties (constructor args):** `child`, `onPressed` (null disables), `color`, `disabledColor`, `padding`, `minSize`, `pressedOpacity` (default 0.4), `borderRadius` (default 8), `alignment`.
**Key methods:** `createState()`.

### CupertinoButton.filled
**Type:** Named constructor of `CupertinoButton`.
**Capabilities:** A filled button whose background is the theme's primary color (white text by default).
**Key constructors / named constructors:** `CupertinoButton.filled({required child, onPressed, padding, disabledColor, minSize, pressedOpacity, borderRadius, alignment})`.
**Key properties (constructor args):** Same as `CupertinoButton` except `color` is fixed to the theme primary.
**Key methods:** Inherited from `CupertinoButton`.

### CupertinoSegmentedControl
**Type:** StatefulWidget (generic `<T>`).
**Capabilities:** Classic iOS segmented control: a row of mutually exclusive, equally sized segments with a bordered/filled selected state.
**Key constructors / named constructors:** `CupertinoSegmentedControl<T>({required children, required onValueChanged, groupValue, unselectedColor, selectedColor, borderColor, pressedColor, padding})`.
**Key properties (constructor args):** `children` (`Map<T, Widget>`), `groupValue`, `onValueChanged`, `selectedColor`, `unselectedColor`, `borderColor`, `pressedColor`, `padding`.
**Key methods:** `createState()`.

### CupertinoSlidingSegmentedControl
**Type:** StatefulWidget (generic `<T>`).
**Capabilities:** Modern iOS sliding segmented control with a thumb that animates between segments on a rounded, filled track.
**Key constructors / named constructors:** `CupertinoSlidingSegmentedControl<T>({required children, required onValueChanged, groupValue, thumbColor, backgroundColor, padding, proportionalWidth})`.
**Key properties (constructor args):** `children` (`Map<T, Widget>`), `groupValue`, `onValueChanged`, `thumbColor`, `backgroundColor`, `padding`.
**Key methods:** `createState()`.

### CupertinoSwitch
**Type:** StatefulWidget.
**Capabilities:** iOS-style on/off toggle switch with animated thumb and track coloring.
**Key constructors / named constructors:** `CupertinoSwitch({required value, required onChanged, activeColor, trackColor, thumbColor, applyTheme, focusColor, dragStartBehavior})`.
**Key properties (constructor args):** `value`, `onChanged` (null disables), `activeColor`, `trackColor`, `thumbColor`, `applyTheme`, `focusColor`, `dragStartBehavior`.
**Key methods:** `createState()`.

### CupertinoSlider
**Type:** StatefulWidget.
**Capabilities:** iOS-style horizontal slider for selecting a continuous (or discrete) value.
**Key constructors / named constructors:** `CupertinoSlider({required value, required onChanged, onChangeStart, onChangeEnd, min = 0.0, max = 1.0, divisions, activeColor, thumbColor})`.
**Key properties (constructor args):** `value`, `min`, `max`, `onChanged`, `onChangeStart`, `onChangeEnd`, `divisions`, `activeColor`, `thumbColor`.
**Key methods:** `createState()`.

### CupertinoActivityIndicator
**Type:** StatefulWidget.
**Capabilities:** iOS-style spinning activity indicator (radial spokes); animated or a static partial-progress wheel.
**Key constructors / named constructors:** `CupertinoActivityIndicator({animating = true, color, radius = 10.0})`; `.partiallyRevealed({color, radius, progress = 1.0})`.
**Key properties (constructor args):** `animating`, `radius`, `color`, `progress` (partial constructor).
**Key methods:** `createState()`.

### CupertinoTextField
**Type:** StatefulWidget.
**Capabilities:** iOS-style single/multi-line text input with rounded border decoration, optional prefix/suffix, clear button, and placeholder.
**Key constructors / named constructors:** `CupertinoTextField({...})`; `.borderless({...})`.
**Key properties (constructor args):** `controller`, `focusNode`, `placeholder`, `placeholderStyle`, `prefix`, `suffix`, `clearButtonMode`, `decoration` (`BoxDecoration`), `padding`, `style`, `textAlign`, `keyboardType`, `textInputAction`, `obscureText`, `maxLines`, `minLines`, `maxLength`, `inputFormatters`, `onChanged`, `onSubmitted`, `onEditingComplete`, `onTap`, `enabled`, `readOnly`, `autofocus`, `cursorColor`.
**Key methods:** `createState()`.

### CupertinoSearchTextField
**Type:** StatefulWidget.
**Capabilities:** iOS-style search box: a rounded field with a leading magnifier icon, "Search" placeholder, and a clear (x) button.
**Key constructors / named constructors:** `CupertinoSearchTextField({controller, onChanged, onSubmitted, placeholder = 'Search', style, decoration, backgroundColor, borderRadius, prefixIcon, suffixIcon, suffixMode, onSuffixTap, itemColor, itemSize, focusNode})`.
**Key properties (constructor args):** `controller`, `focusNode`, `onChanged`, `onSubmitted`, `placeholder`, `prefixIcon`, `suffixIcon`, `suffixMode`, `onSuffixTap`, `backgroundColor`, `borderRadius`, `itemColor`, `itemSize`, `style`, `decoration`.
**Key methods:** `createState()`.

### CupertinoTextFormFieldRow
**Type:** StatefulWidget (a `FormField<String>` subclass / form-aware row).
**Capabilities:** A form-field-aware Cupertino text input formatted as a row (with optional prefix label) for `CupertinoFormSection`; integrates with `Form` validation/saving.
**Key constructors / named constructors:** `CupertinoTextFormFieldRow({prefix, controller, initialValue, validator, onSaved, onChanged, placeholder, decoration, ...})`.
**Key properties (constructor args):** `prefix`, `validator`, `onSaved`, `initialValue`, `controller`, `placeholder`, `decoration`, `padding`, plus most `CupertinoTextField` args.
**Key methods:** `createState()` → `FormFieldState<String>`.

### CupertinoFormSection
**Type:** StatelessWidget.
**Capabilities:** Groups form rows into an iOS Settings-style section with header/footer and inset-grouped or full-width styling.
**Key constructors / named constructors:** `CupertinoFormSection({required children, header, footer, margin, backgroundColor, decoration, clipBehavior})`; `.insetGrouped({...})`.
**Key properties (constructor args):** `children`, `header`, `footer`, `margin`, `backgroundColor`, `decoration`, `clipBehavior`.
**Key methods:** `build(context)`.

### CupertinoFormRow
**Type:** StatelessWidget.
**Capabilities:** A single form row with an optional leading prefix label, a trailing child (control), and helper/error text below.
**Key constructors / named constructors:** `CupertinoFormRow({required child, prefix, padding, helper, error})`.
**Key properties (constructor args):** `child`, `prefix`, `helper`, `error`, `padding`.
**Key methods:** `build(context)`.

### CupertinoListSection
**Type:** StatelessWidget.
**Capabilities:** iOS Settings-style grouped list section containing `CupertinoListTile`s, with header/footer and base or inset-grouped styling.
**Key constructors / named constructors:** `CupertinoListSection({children, header, footer, margin, backgroundColor, decoration, dividerMargin, additionalDividerMargin, topMargin, hasLeading = true, separatorColor})`; `.insetGrouped({...})`.
**Key properties (constructor args):** `children`, `header`, `footer`, `margin`, `backgroundColor`, `decoration`, `dividerMargin`, `additionalDividerMargin`, `topMargin`, `hasLeading`, `separatorColor`.
**Key methods:** `build(context)`.

### CupertinoListTile
**Type:** StatelessWidget.
**Capabilities:** A single iOS-style list row with leading/title/subtitle/trailing slots; tappable, used inside `CupertinoListSection`.
**Key constructors / named constructors:** `CupertinoListTile({required title, subtitle, additionalInfo, leading, trailing, onTap, backgroundColor, backgroundColorActivated, padding, leadingSize, leadingToTitle})`; `.notched({...})`.
**Key properties (constructor args):** `title`, `subtitle`, `additionalInfo`, `leading`, `trailing` (e.g. `CupertinoListTileChevron`), `onTap`, `backgroundColor`, `backgroundColorActivated`, `padding`, `leadingSize`, `leadingToTitle`.
**Key methods:** `build(context)`.

### CupertinoAlertDialog
**Type:** StatelessWidget.
**Capabilities:** iOS-style modal alert dialog with a title, message, and a list of `CupertinoDialogAction` buttons.
**Key constructors / named constructors:** `CupertinoAlertDialog({title, content, actions = const [], scrollController, actionScrollController, insetAnimationDuration, insetAnimationCurve})`.
**Key properties (constructor args):** `title`, `content`, `actions`, `scrollController`, `actionScrollController`.
**Key methods:** `build(context)`.

### CupertinoDialogAction
**Type:** StatelessWidget.
**Capabilities:** A single button within a `CupertinoAlertDialog`/`CupertinoActionSheet`; supports default (bold) and destructive (red) styling.
**Key constructors / named constructors:** `CupertinoDialogAction({child, onPressed, isDefaultAction = false, isDestructiveAction = false, textStyle})`.
**Key properties (constructor args):** `child`, `onPressed` (null disables), `isDefaultAction`, `isDestructiveAction`, `textStyle`.
**Key methods:** `build(context)`.

### CupertinoActionSheet
**Type:** StatelessWidget.
**Capabilities:** iOS-style bottom action sheet with a title/message, action buttons, and an optional separated cancel button.
**Key constructors / named constructors:** `CupertinoActionSheet({title, message, actions, cancelButton, messageScrollController, actionScrollController})`.
**Key properties (constructor args):** `title`, `message`, `actions`, `cancelButton`, `messageScrollController`, `actionScrollController`.
**Key methods:** `build(context)`.

### CupertinoActionSheetAction
**Type:** StatelessWidget.
**Capabilities:** A single button row inside a `CupertinoActionSheet`, with default and destructive styling options.
**Key constructors / named constructors:** `CupertinoActionSheetAction({required child, required onPressed, isDefaultAction = false, isDestructiveAction = false})`.
**Key properties (constructor args):** `child`, `onPressed`, `isDefaultAction`, `isDestructiveAction`.
**Key methods:** `build(context)`.

### showCupertinoDialog
**Type:** Top-level function (returns `Future<T?>`).
**Capabilities:** Displays an iOS-style modal dialog (a `CupertinoDialogRoute`) with the standard fade/scale transition and a translucent barrier.
**Key parameters:** `context`, `builder` (required); `barrierDismissible = false`, `barrierLabel`, `barrierColor`, `useRootNavigator = true`, `routeSettings`, `anchorPoint`.
**Key methods:** n/a (returns the popped result future).

### showCupertinoModalPopup
**Type:** Top-level function (returns `Future<T?>`).
**Capabilities:** Slides a modal popup up from the bottom (e.g. a `CupertinoActionSheet` or `CupertinoPicker`) with a translucent barrier.
**Key parameters:** `context`, `builder` (required); `filter` (`ImageFilter`), `barrierColor`, `barrierDismissible = true`, `useRootNavigator = true`, `semanticsDismissible`, `routeSettings`, `anchorPoint`.
**Key methods:** n/a (returns the popped result future).

### CupertinoContextMenu
**Type:** StatefulWidget.
**Capabilities:** iOS long-press context menu: the child zooms into a fullscreen preview with a list of actions below it.
**Key constructors / named constructors:** `CupertinoContextMenu({required child, required actions, enableHapticFeedback = false})`; `.builder({required builder, required actions})`.
**Key properties (constructor args):** `child`, `actions` (`CupertinoContextMenuAction`s), `enableHapticFeedback`, `builder` (builder constructor).
**Key methods:** `createState()`.

### CupertinoPicker
**Type:** StatefulWidget.
**Capabilities:** iOS-style scrolling wheel selector with 3D perspective; the centered item is selected, with a highlighted selection band.
**Key constructors / named constructors:** `CupertinoPicker({required itemExtent, required onSelectedItemChanged, required children, scrollController, diameterRatio, backgroundColor, offAxisFraction, useMagnifier, magnification, squeeze, selectionOverlay, looping})`; `.builder({required itemExtent, required itemBuilder, childCount, ...})`.
**Key properties (constructor args):** `itemExtent`, `onSelectedItemChanged` (`ValueChanged<int>`), `children`/`itemBuilder`, `scrollController` (`FixedExtentScrollController`), `magnification`, `useMagnifier`, `diameterRatio`, `squeeze`, `offAxisFraction`, `looping`, `backgroundColor`, `selectionOverlay`.
**Key methods:** `createState()`.

### CupertinoDatePicker
**Type:** StatefulWidget.
**Capabilities:** iOS spinning-wheel date and/or time picker supporting several modes (date, time, dateAndTime, monthYear).
**Key constructors / named constructors:** `CupertinoDatePicker({required onDateTimeChanged, mode = CupertinoDatePickerMode.dateAndTime, initialDateTime, minimumDate, maximumDate, minimumYear = 1, maximumYear, minuteInterval = 1, use24hFormat = false, dateOrder, backgroundColor, showDayOfWeek, itemExtent})`.
**Key properties (constructor args):** `mode`, `onDateTimeChanged`, `initialDateTime`, `minimumDate`, `maximumDate`, `minimumYear`, `maximumYear`, `minuteInterval`, `use24hFormat`, `dateOrder`, `showDayOfWeek`, `backgroundColor`, `itemExtent`.
**Key methods:** `createState()`.

### CupertinoTimerPicker
**Type:** StatefulWidget.
**Capabilities:** iOS countdown-timer-style wheel picker for selecting a `Duration` (hours/minutes/seconds).
**Key constructors / named constructors:** `CupertinoTimerPicker({required onTimerDurationChanged, mode = CupertinoTimerPickerMode.hms, initialTimerDuration = Duration.zero, minuteInterval = 1, secondInterval = 1, alignment, backgroundColor, itemExtent})`.
**Key properties (constructor args):** `mode`, `onTimerDurationChanged`, `initialTimerDuration`, `minuteInterval`, `secondInterval`, `alignment`, `backgroundColor`, `itemExtent`.
**Key methods:** `createState()`.

### CupertinoPageTransition
**Type:** StatelessWidget (a transition widget).
**Capabilities:** The iOS slide-from-right page transition (with parallax of the outgoing page); used internally by `CupertinoPageRoute`.
**Key constructors / named constructors:** `CupertinoPageTransition({required primaryRouteAnimation, required secondaryRouteAnimation, required child, required linearTransition})`.
**Key properties (constructor args):** `primaryRouteAnimation`, `secondaryRouteAnimation`, `child`, `linearTransition`.
**Key methods:** `build(context)`.

### CupertinoFullscreenDialogTransition
**Type:** StatelessWidget (a transition widget).
**Capabilities:** The iOS bottom-up slide transition for fullscreen modal dialog routes.
**Key constructors / named constructors:** `CupertinoFullscreenDialogTransition({required primaryRouteAnimation, required secondaryRouteAnimation, required child, required linearTransition})`.
**Key properties (constructor args):** `primaryRouteAnimation`, `secondaryRouteAnimation`, `child`, `linearTransition`.
**Key methods:** `build(context)`.

### CupertinoScrollbar
**Type:** StatefulWidget (extends `RawScrollbar`).
**Capabilities:** iOS-style scrollbar that fades in during scrolling and supports drag-to-scroll on the thumb.
**Key constructors / named constructors:** `CupertinoScrollbar({required child, controller, thumbVisibility, thickness = 3.0, thicknessWhileDragging = 8.0, radius, radiusWhileDragging, scrollbarOrientation})`.
**Key properties (constructor args):** `child`, `controller`, `thumbVisibility`, `thickness`, `thicknessWhileDragging`, `radius`, `radiusWhileDragging`, `scrollbarOrientation`.
**Key methods:** `createState()`.

### CupertinoPageRoute (supporting)
**Type:** `PageRoute<T>` subclass (modal route).
**Capabilities:** A route using the iOS page transition (slide-from-right with back-swipe support); the Cupertino analog of `MaterialPageRoute`.
**Key constructors / named constructors:** `CupertinoPageRoute({required builder, title, settings, maintainState = true, fullscreenDialog = false, allowSnapshotting = true})`.
**Key properties (constructor args):** `builder`, `title` (back-button previous-page title), `maintainState`, `fullscreenDialog`, `settings`.
**Key methods:** `buildPage(...)`, `buildTransitions(...)`; static `CupertinoRouteTransitionMixin.buildPageTransitions(...)`.

### CupertinoIcons (supporting)
**Type:** Class of `static const IconData` constants (icon font glyphs).
**Capabilities:** Provides built-in iOS-style glyphs (e.g. `CupertinoIcons.back`, `.add`, `.search`, `.share`, `.settings`, `.heart`) for use with `Icon`.
**Key properties:** static `IconData` fields referencing the CupertinoIcons font family.
**Key methods:** n/a.

## Inherited / utility / app-config widgets

### Theme
**Type:** StatelessWidget (wraps an `InheritedWidget`).
**Capabilities:** Applies a Material `ThemeData` to its subtree so descendants can read colors, typography, and component themes; animates between themes when changed.
**Key constructors / named constructors:** `Theme({required data, required child})`.
**Key properties (constructor args):** `data` (`ThemeData`), `child`.
**Key methods:** `static ThemeData of(BuildContext)`; `build(context)`.

### ThemeData (supporting)
**Type:** Configuration data class (`@immutable`, not a widget).
**Capabilities:** Holds the entire Material theme: color scheme, text theme, brightness, and per-component sub-themes; passed to `Theme`/`MaterialApp.theme`.
**Key constructors / named constructors:** `ThemeData({brightness, colorScheme, primaryColor, textTheme, useMaterial3, ...})`; `.light()`, `.dark()`, `.from({required colorScheme, ...})`, `.fallback()`.
**Key properties (constructor args):** `brightness`, `colorScheme`, `primaryColor`, `scaffoldBackgroundColor`, `textTheme`, `primaryTextTheme`, `iconTheme`, `useMaterial3`, plus component themes (`appBarTheme`, `cardTheme`, `buttonTheme`, ...).
**Key methods:** `copyWith(...)`, `lerp(a, b, t)`.

### CupertinoTheme
**Type:** StatelessWidget (wraps an `InheritedWidget`).
**Capabilities:** Applies a `CupertinoThemeData` to descendants, defining iOS colors and text styles; the Cupertino analog of `Theme`.
**Key constructors / named constructors:** `CupertinoTheme({required data, required child})`.
**Key properties (constructor args):** `data`, `child`.
**Key methods:** `static CupertinoThemeData of(BuildContext)`; `static Brightness? brightnessOf(BuildContext)`; `build(context)`.

### MediaQuery
**Type:** InheritedWidget (`InheritedModel<_MediaQueryAspect>`).
**Capabilities:** Exposes `MediaQueryData` (screen size, padding, insets, text scaling, platform brightness, accessibility flags) and triggers rebuilds when the relevant aspect changes.
**Key constructors / named constructors:** `MediaQuery({required data, required child})`; `.removePadding({...})`, `.removeViewInsets({...})`, `.removeViewPadding({...})`.
**Key properties (constructor args):** `data`, `child`.
**Key methods:** `static MediaQueryData of(BuildContext)`; `maybeOf`; aspect accessors `sizeOf`, `paddingOf`, `textScalerOf`, `platformBrightnessOf`, `viewInsetsOf`.

### MediaQueryData (supporting)
**Type:** Configuration data class (`@immutable`, not a widget).
**Capabilities:** Immutable snapshot of media properties: size, devicePixelRatio, padding, insets, text scaling, brightness, accessibility settings.
**Key constructors / named constructors:** `MediaQueryData({size, devicePixelRatio, textScaler, padding, viewInsets, ...})`; `.fromView(view, {platformData})`.
**Key properties (constructor args):** `size`, `devicePixelRatio`, `textScaler` (replaces deprecated `textScaleFactor`), `padding`, `viewInsets`, `viewPadding`, `systemGestureInsets`, `platformBrightness`, `orientation`, `disableAnimations`, `boldText`, `highContrast`, `accessibleNavigation`, `invertColors`.
**Key methods:** `copyWith(...)`, `removePadding(...)`, `removeViewInsets(...)`, `removeViewPadding(...)`.

### Directionality
**Type:** InheritedWidget.
**Capabilities:** Establishes the ambient `TextDirection` (LTR/RTL) for its subtree, used by text, layout (`Row`, `Padding.directional`), and bidirectional widgets.
**Key constructors / named constructors:** `Directionality({required textDirection, required child})`.
**Key properties (constructor args):** `textDirection`, `child`.
**Key methods:** `static TextDirection of(BuildContext)`; `static TextDirection? maybeOf(BuildContext)`.

### DefaultTextStyle (cross-ref)
**Type:** InheritedWidget. *(Full detail in the text section.)*
**Capabilities:** Provides the default `TextStyle` (and alignment/overflow/maxLines/softWrap) for descendant `Text` widgets that don't specify their own.
**Key constructors / named constructors:** `DefaultTextStyle({required style, textAlign, softWrap, overflow, maxLines, required child})`; `.merge({...})`.
**Key properties (constructor args):** `style`, `textAlign`, `softWrap`, `overflow`, `maxLines`, `child`.
**Key methods:** `static DefaultTextStyle of(BuildContext)`.

### IconTheme
**Type:** InheritedWidget (`InheritedTheme`).
**Capabilities:** Supplies default `IconThemeData` (color, size, opacity) to descendant `Icon` widgets.
**Key constructors / named constructors:** `IconTheme({required data, required child})`; `.merge({required data, required child})`.
**Key properties (constructor args):** `data` (`IconThemeData`: color, size, opacity, fill, weight, grade), `child`.
**Key methods:** `static IconThemeData of(BuildContext)`.

### Banner
**Type:** StatelessWidget (paints via a `CustomPainter`).
**Capabilities:** Draws a diagonal ribbon/banner (e.g. "DEBUG"/"BETA") across a corner of its child. `CheckedModeBanner` (and `debugShowCheckedModeBanner`) uses it. Distinct from `MaterialBanner`.
**Key constructors / named constructors:** `Banner({required message, required location, child, textDirection, color, layoutDirection, textStyle})`.
**Key properties (constructor args):** `message`, `location` (`BannerLocation`), `color`, `textStyle`, `textDirection`, `layoutDirection`, `child`.
**Key methods:** `build(context)`.

### WidgetsApp (mention)
**Type:** StatefulWidget.
**Capabilities:** The lowest-level application widget wrapping `Navigator`, `Localizations`, `MediaQuery`, default text/icon styling, and routing; `MaterialApp` and `CupertinoApp` build on it.
**Key constructors / named constructors:** `WidgetsApp({...})`, `.router({...})`.
**Key properties (constructor args):** `home`/`routes`/`onGenerateRoute`, `pageRouteBuilder` (required with `routes`/`home`), `color` (required), `title`, `localizationsDelegates`, `supportedLocales`, `navigatorKey`, `builder`, router equivalents.
**Key methods:** `createState()`.

### Localizations
**Type:** StatefulWidget.
**Capabilities:** Loads and provides locale-specific resources (from `LocalizationsDelegate`s) to its subtree and establishes the ambient `Locale`.
**Key constructors / named constructors:** `Localizations({required locale, required delegates, child})`; `.override({required context, locale, delegates, required child})`.
**Key properties (constructor args):** `locale`, `delegates`, `child`.
**Key methods:** `static Locale? localeOf(BuildContext)`; `static T? of<T>(BuildContext, Type)`; `createState()`.

### DefaultAssetBundle
**Type:** InheritedWidget.
**Capabilities:** Provides the ambient `AssetBundle` used to load assets for the subtree; defaults to `rootBundle` when absent.
**Key constructors / named constructors:** `DefaultAssetBundle({required bundle, required child})`.
**Key properties (constructor args):** `bundle`, `child`.
**Key methods:** `static AssetBundle of(BuildContext)`.

### Title
**Type:** StatelessWidget.
**Capabilities:** Describes the application to the OS (title string and primary `color`) — e.g. the Android task-switcher label.
**Key constructors / named constructors:** `Title({required color, title = '', required child, textDirection})`.
**Key properties (constructor args):** `title`, `color` (must be fully opaque), `textDirection`, `child`.
**Key methods:** `build(context)`.

### ScrollConfiguration (cross-ref)
**Type:** InheritedWidget. *(Full detail in the scrolling section.)*
**Capabilities:** Provides a `ScrollBehavior` to descendant scrollables (overscroll glow/bounce, scrollbar presence, multitouch drag, accepted pointer devices).
**Key constructors / named constructors:** `ScrollConfiguration({required behavior, required child})`.
**Key properties (constructor args):** `behavior`, `child`.
**Key methods:** `static ScrollBehavior of(BuildContext)`.

### PrimaryScrollController
**Type:** InheritedWidget.
**Capabilities:** Provides a `ScrollController` that scroll views can automatically attach to (e.g. tap-status-bar-to-scroll-to-top on iOS, implicit controller sharing).
**Key constructors / named constructors:** `PrimaryScrollController({required controller, automaticallyInheritForPlatforms, scrollDirection, required child})`; `.none({required child})`.
**Key properties (constructor args):** `controller`, `automaticallyInheritForPlatforms`, `scrollDirection`, `child`.
**Key methods:** `static ScrollController? of(BuildContext)`; `maybeOf`; `static bool shouldInherit(BuildContext, Axis)`.

### AnimatedTheme
**Type:** ImplicitlyAnimatedWidget (StatefulWidget).
**Capabilities:** Animates the transition between `ThemeData` values over a duration/curve; used internally by `Theme`.
**Key constructors / named constructors:** `AnimatedTheme({required data, curve = Curves.linear, required duration, onEnd, required child})`.
**Key properties (constructor args):** `data`, `duration`, `curve`, `onEnd`, `child`.
**Key methods:** `createState()` (drives a `ThemeDataTween`).

### Overlay
**Type:** StatefulWidget.
**Capabilities:** A stack of independently managed `OverlayEntry`s rendered above the rest of the UI; the foundation for routes, tooltips, dropdowns, drag feedback, and selection handles.
**Key constructors / named constructors:** `Overlay({initialEntries = const [], clipBehavior = Clip.hardEdge, key})`; `.wrap({required child})`.
**Key properties (constructor args):** `initialEntries`, `clipBehavior`.
**Key methods:** `static OverlayState of(BuildContext, {rootOverlay})`; `maybeOf`; `createState()` → `OverlayState`.

### OverlayEntry (supporting)
**Type:** Controller/data class (a `Listenable`, not a widget).
**Capabilities:** Represents a single entry (a builder) within an `Overlay`; can be inserted, removed, reordered, and marked to rebuild; can be `opaque` and `maintainState`.
**Key constructors / named constructors:** `OverlayEntry({required builder, opaque = false, maintainState = false, canSizeOverlay = false})`.
**Key properties (constructor args):** `builder`, `opaque`, `maintainState`.
**Key methods:** `remove()`, `markNeedsBuild()`, `dispose()`; via `OverlayState`: `insert(entry, {above, below})`, `insertAll(...)`, `rearrange(...)`.

### HeroControllerScope (mention)
**Type:** InheritedWidget.
**Capabilities:** Provides a `HeroController` to a `Navigator` subtree so `Hero` animations run during route transitions; `MaterialApp`/`CupertinoApp` install one automatically.
**Key constructors / named constructors:** `HeroControllerScope({required controller, required child})`; `.none({required child})`.
**Key properties (constructor args):** `controller`, `child`.
**Key methods:** `static HeroController? of(BuildContext)`; `maybeOf`.
