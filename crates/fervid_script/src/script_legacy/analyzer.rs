use swc_core::ecma::{
    ast::{
        ArrayLit, ArrowExpr, BlockStmtOrExpr, Expr, Function, Lit, Module, ModuleDecl, ModuleItem,
        ObjectLit, Prop, PropName, PropOrSpread, Tpl,
    },
    atoms::JsWord,
};

use crate::{
    atoms::*, common::utils::get_string_tpl, script_legacy::ScriptLegacyVars, setup_analyzer,
    structs::VueResolvedImports,
};

use super::{
    components::collect_components_object,
    computed::collect_computed_object,
    data::{collect_data_bindings_block_stmt, collect_data_bindings_expr},
    directives::collect_directives_object,
    emits::{collect_emits_bindings_array, collect_emits_bindings_object},
    expose::collect_expose_bindings_array,
    inject::{collect_inject_bindings_array, collect_inject_bindings_object},
    methods::collect_methods_object,
    props::{collect_prop_bindings_array, collect_prop_bindings_object},
    setup::{collect_setup_bindings_block_stmt, collect_setup_bindings_expr},
    exports::{collect_exports_decl, collect_exports_named},
};

/// Analyzes all the fields of `export default` according to Options API.\
/// tl;dr Visit every method, arrow function, object or array and forward control
pub fn analyze_default_export(default_export: &ObjectLit, out: &mut ScriptLegacyVars) {
    for field in default_export.props.iter() {
        let PropOrSpread::Prop(prop) = field else {
            continue;
        };

        match **prop {
            Prop::KeyValue(ref key_value) => {
                let sym = match key_value.key {
                    PropName::Ident(ref ident) => &ident.sym,
                    PropName::Str(ref s) => &s.value,
                    _ => continue,
                };

                match *key_value.value {
                    Expr::Array(ref array_lit) => handle_options_array(sym, array_lit, out),
                    Expr::Object(ref obj_lit) => handle_options_obj(sym, obj_lit, out),
                    Expr::Fn(ref fn_expr) => handle_options_function(sym, &fn_expr.function, out),
                    Expr::Arrow(ref arrow_expr) => {
                        handle_options_arrow_function(sym, arrow_expr, out)
                    }
                    Expr::Lit(ref lit) => handle_options_lit(sym, lit, out),
                    Expr::Tpl(ref tpl) => handle_options_tpl(sym, tpl, out),

                    // These latter types technically can be analyzed as well,
                    // because they only need `.expr` unwrapping and re-matching.
                    // It can be done when the match moves into a function
                    // which can be recursively called.
                    // Expr::TsTypeAssertion(_) => todo!(),
                    // Expr::TsConstAssertion(_) => todo!(),
                    // Expr::TsAs(_) => todo!(),
                    _ => {
                        continue;
                    }
                }
            }
            Prop::Method(ref method) => {
                let sym = match method.key {
                    PropName::Ident(ref ident) => &ident.sym,
                    PropName::Str(ref s) => &s.value,
                    _ => continue,
                };

                handle_options_function(sym, &method.function, out)
            }
            _ => {}
        }
    }
}

/// Analyzes the top level statements in dual-script mode,
/// i.e. when both `<script>` and `<script setup>` are present.
pub fn analyze_top_level_items(
    module: &Module,
    out: &mut ScriptLegacyVars,
    vue_imports: &mut VueResolvedImports,
) {
    for module_item in module.body.iter() {
        match *module_item {
            ModuleItem::ModuleDecl(ref module_decl) => {
                match module_decl {
                    ModuleDecl::Import(ref import_decl) => {
                        setup_analyzer::collect_imports(import_decl, &mut out.imports, vue_imports)
                    },

                    ModuleDecl::ExportNamed(ref named_exports) => {
                        collect_exports_named(named_exports, &mut out.setup)
                    }

                    ModuleDecl::ExportDecl(ref export_decl) => {
                        collect_exports_decl(export_decl, &mut out.setup, vue_imports)
                    }

                    // Other types are ignored (ModuleDecl::Export* and ModuleDecl::Ts*)
                    _ => {}
                }
            }

            ModuleItem::Stmt(ref stmt) => {
                setup_analyzer::analyze_stmt(stmt, &mut out.setup, vue_imports)
            }
        }
    }
}

/// In Options API, `props`, `inject`, `emits` and `expose` may be arrays
fn handle_options_array(
    field: &JsWord,
    array_lit: &ArrayLit,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    if *field == *PROPS {
        collect_prop_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *INJECT {
        collect_inject_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *EMITS {
        collect_emits_bindings_array(array_lit, script_legacy_vars)
    } else if *field == *EXPOSE {
        collect_expose_bindings_array(array_lit, script_legacy_vars)
    }
}

/// Similar to [handle_options_array], only `data`, `setup` may be declared as arrow fns
fn handle_options_arrow_function(
    field: &JsWord,
    arrow_expr: &ArrowExpr,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    // Arrow functions may either have a body or an expression
    // `() => {}` is a body which returns nothing
    // `() => ({})` is an expression which returns an empty object
    macro_rules! forward_block_stmt_or_expr {
        ($forward_block_stmt: ident, $forward_expr: ident) => {
            match *arrow_expr.body {
                BlockStmtOrExpr::BlockStmt(ref block_stmt) => {
                    $forward_block_stmt(block_stmt, script_legacy_vars)
                }
                BlockStmtOrExpr::Expr(ref arrow_body_expr) => {
                    $forward_expr(arrow_body_expr, script_legacy_vars)
                }
            }
        };
    }

    // It reads a bit cryptic because of the macro calls,
    // but you should only care about the functions which are called,
    // e.g. [`collect_data_bindings_block_stmt`]
    if *field == *DATA {
        forward_block_stmt_or_expr!(collect_data_bindings_block_stmt, collect_data_bindings_expr);
    } else if *field == *SETUP {
        forward_block_stmt_or_expr!(
            collect_setup_bindings_block_stmt,
            collect_setup_bindings_expr
        )
    }
}

/// Same as [handle_options_arrow_function], `data` and `setup`
fn handle_options_function(
    field: &JsWord,
    function: &Function,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    let Some(ref function_body) = function.body else {
        return;
    };

    if *field == *DATA {
        collect_data_bindings_block_stmt(function_body, script_legacy_vars)
    } else if *field == *SETUP {
        collect_setup_bindings_block_stmt(function_body, script_legacy_vars)
    }
}

/// `name`
fn handle_options_lit(field: &JsWord, lit: &Lit, script_legacy_vars: &mut ScriptLegacyVars) {
    if *field == *NAME {
        if let Lit::Str(name) = lit {
            script_legacy_vars.name = Some(name.value.to_owned())
        }
    }
}

/// `name`
fn handle_options_tpl(field: &JsWord, tpl: &Tpl, script_legacy_vars: &mut ScriptLegacyVars) {
    if *field == *NAME {
        script_legacy_vars.name = get_string_tpl(tpl);
    }
}

/// `props`, `computed`, `inject`, `emits`, `components`, `methods`, `directives`
fn handle_options_obj(
    field: &JsWord,
    obj_lit: &ObjectLit,
    script_legacy_vars: &mut ScriptLegacyVars,
) {
    if *field == *PROPS {
        collect_prop_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *COMPUTED {
        collect_computed_object(obj_lit, script_legacy_vars)
    } else if *field == *INJECT {
        collect_inject_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *EMITS {
        collect_emits_bindings_object(obj_lit, script_legacy_vars)
    } else if *field == *COMPONENTS {
        collect_components_object(obj_lit, script_legacy_vars)
    } else if *field == *METHODS {
        collect_methods_object(obj_lit, script_legacy_vars)
    } else if *field == *DIRECTIVES {
        collect_directives_object(obj_lit, script_legacy_vars)
    }
}
