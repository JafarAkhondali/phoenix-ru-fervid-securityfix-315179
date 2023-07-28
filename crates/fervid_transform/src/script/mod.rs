use fervid_core::SfcScriptBlock;
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        Callee, Expr, ExprOrSpread, Module, ModuleDecl, ModuleItem,
        ObjectLit, PropOrSpread, SpreadElement,
    },
};

use crate::template::ScopeHelper;

use self::setup::transform_and_record_script_setup;

mod options;
mod setup;

pub fn transform_and_record_scripts(
    script_setup: Option<SfcScriptBlock>,
    script_legacy: Option<SfcScriptBlock>,
    scope_helper: &mut ScopeHelper,
) -> (Module, ObjectLit) {
    let mut module_base: Module = script_legacy.map_or_else(
        || Module {
            span: DUMMY_SP,
            body: vec![],
            shebang: None,
        },
        |script| *script.content,
    );

    let mut default_export = get_default_export_obj(&mut module_base);

    // TODO Process `default_export` by collecting variables (use `fervid_script` code)

    if let Some(script_setup) = script_setup {
        let setup_transform_result = transform_and_record_script_setup(script_setup, scope_helper);

        // TODO Push imports at module top or bottom? Or smart merge?

        // TODO Adding bindings to `setup()` in Options API will get overwritten in `<script setup>`
        // https://play.vuejs.org/#eNp9U01v2zAM/SuELm6BNFmTm5F22IYetsM2bMUudTEYNp2okyVDklMPQf77SNpunS7txTQfH/n4Ye/Vh6aZ71pUqVpHrBuTR7zOLAB5IV4Urm7EFaAPw+5CV1eZir7FTA1RgMq5gbg4KnScGYyLKVGf0rb6ZBa7z/pDQ//rB2qA7cvs7ZJYaAL21CqnV6KKXS+2y4G1GljX/CB8NWqVekehynlK/g3awipTBBRtiK7mMbbucVJ3vaCEMZdHBJvXSAQ2pRAYPTFJL3F2pwm7nAGb5T1ZW2J3zsJGh0gF9nuJXcLhcDQr16OYa6J2NlB0kNC2aSPVr12JhhTE/soNnwzS+Lfh7qR9eA9JxC4mkEJSUtVERp3ujetg7Qi4o9PdC+BswfovmlmHwusmQsDY8uF03TgfgW/5iU4Jlaf1JXM5Ln92CScV1HmE25FzBQnBtDEpNS1L79hJwRKrvDUR9jysiJ2d9w6AJ9fb0YNxNynIBysgbUkesq1ePifddxNZNVMxUKjSm/lDcJZ+EKmYKf4mtUH/ra+bqXTUylRujHv8IhirzUa82GLx5wT+EDrGMvXdY0C/o2U/xWLuN0i35/DNz690okmQ7tkaYr8R/IHBmZZ77GkfW1tS2xOedPtZTqTt5jbcdBFtGIca13UQfqboXHyf10Z/bnc1X437VYd/HFh0XQ==

        // Merge fields into an SFC exported object
        default_export.props.extend(setup_transform_result.fields);
    }

    (module_base, default_export)
}

/// Finds and takes ownership of the `export default` expression.
/// If a Module misses one, creates an empty ObjectLit
fn get_default_export_obj(module: &mut Module) -> ObjectLit {
    let default_export_index = module
        .body
        .iter()
        .position(|module_item| match module_item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(_)) => true,
            _ => false,
        });

    let Some(idx) = default_export_index else {
        // {}
        return ObjectLit {
            span: DUMMY_SP,
            props: vec![],
        };
    };

    let item = module.body.remove(idx);
    // TODO What to do with weird default exports?
    let ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(expr)) = item else { unreachable!() };

    // TODO Unroll paren/seq, unroll `defineComponent` as in `fervid_script`
    let expr = unroll_default_export_expr(*expr.expr);

    // { /* object fields */ }
    if let Expr::Object(obj_lit) = expr {
        return obj_lit;
    }

    // { ...expression }
    ObjectLit {
        span: DUMMY_SP,
        props: vec![PropOrSpread::Spread(SpreadElement {
            dot3_token: DUMMY_SP,
            expr: Box::new(expr),
        })],
    }
}

fn unroll_default_export_expr(mut expr: Expr) -> Expr {
    match expr {
        Expr::Call(ref mut call_expr) => {
            macro_rules! bail {
                () => {
                    return expr;
                };
            }

            // We only support `defineComponent` with 1 argument which isn't a spread
            if call_expr.args.len() != 1 {
                bail!();
            }

            let Callee::Expr(ref callee) = call_expr.callee else {
                bail!();
            };

            let Expr::Ident(callee_ident) = callee.as_ref() else {
                bail!();
            };

            // Todo compare against the imported symbol
            if &callee_ident.sym != "defineComponent" {
                bail!();
            }

            let is_first_arg_ok = matches!(call_expr.args[0], ExprOrSpread { spread: None, .. });
            if !is_first_arg_ok {
                bail!();
            }

            let Some(ExprOrSpread { spread: None, expr }) = call_expr.args.pop() else {
                unreachable!()
            };

            *expr
        }

        _ => expr,
    }
}
