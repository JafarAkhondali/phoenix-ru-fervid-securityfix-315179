use fervid_script::structs::{SetupBinding, ScriptLegacyVars};
use smallvec::SmallVec;
use swc_core::ecma::atoms::JsWord;

#[derive(Debug)]
pub struct TemplateScope {
    pub variables: SmallVec<[JsWord; 1]>,
    pub parent: u32,
}

#[derive(Debug, Default)]
pub struct ScopeHelper {
    pub template_scopes: Vec<TemplateScope>,
    pub setup_bindings: Vec<SetupBinding>,
    pub options_api_vars: Box<ScriptLegacyVars>,
    pub is_inline: bool,
    pub transform_mode: TemplateGenerationMode
}

#[derive(Debug, Default)]
pub enum TemplateGenerationMode {
    /// Applies the transformation as if the template is rendered inline
    /// and variables are directly accessible in the function scope.
    /// For example, if there is `const foo = ref(0)`, then `foo` will be transformed to `foo.value`.
    /// Non-ref bindings and literal constants will remain untouched.
    Inline,

    /// Applies the transformation as if the template is inside a
    /// `function render(_ctx, _cache, $props, $setup, $data, $options)`.\
    /// Variable access will be translated to object property access,
    /// e.g. `const foo = ref(0)` and `foo.bar` -> `$setup.foo.bar`.
    #[default]
    RenderFn
}
