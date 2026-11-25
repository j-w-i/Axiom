# 0009 Flutter, widgets layer only, own design system
**Decision.** No Material, Cupertino or state framework. Design tokens in one file; state in plain `Listenable`s at the smallest scope that changes.
**Evidence.** Stock widgets cannot express the language (`../design/`); Flutter 3.50 has moved Material out of the core package. The component set so far is under 2 000 lines.
**Trade-off.** We own text fields, menus, focus and semantics; tested per component. **Revisit.** If accessibility or IME parity lags the platform.
