// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-11

//! Template rendering for output format generation.
//!
//! Converts `TemplateDecl` into structured text instructions that get
//! appended to tool descriptions, telling the LLM exactly what format
//! to return its output in.

use crate::ast::expr::ExprKind;
use crate::ast::stmt::{DirectiveDecl, TemplateDecl, TemplateEntry};

/// Render a template into output format instructions for an LLM.
///
/// For a template like:
/// ```pact
/// template %website_copy {
///     HERO_TAGLINE :: String  <<one powerful headline>>
///     MENU_ITEM :: String * 3 <<Name | Price>>
/// }
/// ```
///
/// Generates:
/// ```text
/// You MUST return output in this exact format:
///
/// HERO_TAGLINE: (one powerful headline)
/// MENU_ITEM_1: (Name | Price)
/// MENU_ITEM_2: (Name | Price)
/// MENU_ITEM_3: (Name | Price)
/// ```
pub fn render_template(template: &TemplateDecl) -> String {
    let mut out = String::new();
    out.push_str("You MUST return output in this exact format:\n\n");

    for entry in &template.entries {
        match entry {
            TemplateEntry::Field { name, description, .. } => {
                if let Some(desc) = description {
                    out.push_str(&format!("{}: ({})\n", name, desc.trim()));
                } else {
                    out.push_str(&format!("{}:\n", name));
                }
            }
            TemplateEntry::Repeat { name, count, description, .. } => {
                for i in 1..=*count {
                    if let Some(desc) = description {
                        out.push_str(&format!("{}_{}: ({})\n", name, i, desc.trim()));
                    } else {
                        out.push_str(&format!("{}_{}:\n", name, i));
                    }
                }
            }
            TemplateEntry::Section { name, description } => {
                out.push_str(&format!("\n==={}===\n", name));
                if let Some(desc) = description {
                    out.push_str(&format!("({})\n", desc.trim()));
                }
            }
        }
    }

    out
}

/// Render a directive into prompt text, substituting default parameter values.
///
/// For a directive like:
/// ```pact
/// directive %scandinavian_design {
///     <<Use Google Fonts ({heading_font} for headings, {body_font} for body).>>
///     params {
///         heading_font :: String = "Playfair Display"
///         body_font :: String = "Inter"
///     }
/// }
/// ```
///
/// Generates:
/// ```text
/// Use Google Fonts (Playfair Display for headings, Inter for body).
/// ```
pub fn render_directive(directive: &DirectiveDecl) -> String {
    let mut text = directive.text.trim().to_string();

    // Substitute default parameter values
    for param in &directive.params {
        let default_value = match &param.default.kind {
            ExprKind::StringLit(s) => s.clone(),
            ExprKind::PromptLit(s) => s.clone(),
            ExprKind::IntLit(n) => n.to_string(),
            ExprKind::FloatLit(f) => f.to_string(),
            ExprKind::BoolLit(b) => b.to_string(),
            _ => format!("{{{}}}", param.name),
        };
        text = text.replace(&format!("{{{}}}", param.name), &default_value);
    }

    text
}

/// Render multiple directives into a combined prompt section.
pub fn render_directives(directives: &[&DirectiveDecl]) -> String {
    if directives.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for (i, directive) in directives.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&render_directive(directive));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::types::{TypeExpr, TypeExprKind};
    use crate::span::{SourceId, Span};

    fn string_type() -> TypeExpr {
        TypeExpr {
            kind: TypeExprKind::Named("String".into()),
            span: Span::new(SourceId(0), 0, 0),
        }
    }

    #[test]
    fn render_simple_fields() {
        let template = TemplateDecl {
            name: "test".into(),
            entries: vec![
                TemplateEntry::Field {
                    name: "TITLE".into(),
                    ty: string_type(),
                    description: Some("a catchy title".into()),
                },
                TemplateEntry::Field {
                    name: "BODY".into(),
                    ty: string_type(),
                    description: Some("main content".into()),
                },
            ],
        };
        let rendered = render_template(&template);
        assert!(rendered.contains("TITLE: (a catchy title)"));
        assert!(rendered.contains("BODY: (main content)"));
    }

    #[test]
    fn render_repeat_expands() {
        let template = TemplateDecl {
            name: "test".into(),
            entries: vec![
                TemplateEntry::Repeat {
                    name: "ITEM".into(),
                    ty: string_type(),
                    count: 3,
                    description: Some("Name | Price".into()),
                },
            ],
        };
        let rendered = render_template(&template);
        assert!(rendered.contains("ITEM_1: (Name | Price)"));
        assert!(rendered.contains("ITEM_2: (Name | Price)"));
        assert!(rendered.contains("ITEM_3: (Name | Price)"));
        assert!(!rendered.contains("ITEM_4"));
    }

    #[test]
    fn render_sections() {
        let template = TemplateDecl {
            name: "bilingual".into(),
            entries: vec![
                TemplateEntry::Section {
                    name: "ENGLISH".into(),
                    description: Some("original English copy".into()),
                },
                TemplateEntry::Section {
                    name: "SWEDISH".into(),
                    description: Some("translated Swedish copy".into()),
                },
            ],
        };
        let rendered = render_template(&template);
        assert!(rendered.contains("===ENGLISH==="));
        assert!(rendered.contains("(original English copy)"));
        assert!(rendered.contains("===SWEDISH==="));
        assert!(rendered.contains("(translated Swedish copy)"));
    }

    #[test]
    fn render_mixed_template() {
        let template = TemplateDecl {
            name: "website_copy".into(),
            entries: vec![
                TemplateEntry::Field {
                    name: "HERO_TAGLINE".into(),
                    ty: string_type(),
                    description: Some("one powerful headline".into()),
                },
                TemplateEntry::Repeat {
                    name: "MENU_ITEM".into(),
                    ty: string_type(),
                    count: 2,
                    description: Some("Name | Price | Description".into()),
                },
            ],
        };
        let rendered = render_template(&template);
        assert!(rendered.contains("HERO_TAGLINE: (one powerful headline)"));
        assert!(rendered.contains("MENU_ITEM_1: (Name | Price | Description)"));
        assert!(rendered.contains("MENU_ITEM_2: (Name | Price | Description)"));
    }

    #[test]
    fn render_field_no_description() {
        let template = TemplateDecl {
            name: "test".into(),
            entries: vec![
                TemplateEntry::Field {
                    name: "CONTENT".into(),
                    ty: string_type(),
                    description: None,
                },
            ],
        };
        let rendered = render_template(&template);
        assert!(rendered.contains("CONTENT:"));
    }

    // -- directive rendering tests ------------------------------------------

    use crate::ast::expr::Expr;
    use crate::ast::stmt::{DirectiveDecl, DirectiveParam};

    #[test]
    fn render_directive_simple() {
        let directive = DirectiveDecl {
            name: "test".into(),
            text: "Use beautiful animations.".into(),
            params: vec![],
        };
        let rendered = render_directive(&directive);
        assert_eq!(rendered, "Use beautiful animations.");
    }

    #[test]
    fn render_directive_with_defaults() {
        let directive = DirectiveDecl {
            name: "design".into(),
            text: "Use {font} for headings and {color} palette.".into(),
            params: vec![
                DirectiveParam {
                    name: "font".into(),
                    ty: string_type(),
                    default: Expr {
                        kind: ExprKind::StringLit("Playfair Display".into()),
                        span: Span::new(SourceId(0), 0, 0),
                    },
                },
                DirectiveParam {
                    name: "color".into(),
                    ty: string_type(),
                    default: Expr {
                        kind: ExprKind::StringLit("warm".into()),
                        span: Span::new(SourceId(0), 0, 0),
                    },
                },
            ],
        };
        let rendered = render_directive(&directive);
        assert_eq!(rendered, "Use Playfair Display for headings and warm palette.");
    }

    #[test]
    fn render_multiple_directives() {
        let d1 = DirectiveDecl {
            name: "a".into(),
            text: "First block.".into(),
            params: vec![],
        };
        let d2 = DirectiveDecl {
            name: "b".into(),
            text: "Second block.".into(),
            params: vec![],
        };
        let rendered = render_directives(&[&d1, &d2]);
        assert!(rendered.contains("First block."));
        assert!(rendered.contains("Second block."));
        assert!(rendered.contains("\n\n"));
    }
}
