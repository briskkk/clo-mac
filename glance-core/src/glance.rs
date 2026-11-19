//! 菜单栏左键概览要画的行。数字只来自已经解析好的服务端状态，缺字段不补 0。

use crate::menu::{BarState, TaskSource};
use crate::money::{format_cny, Tone};

/// 原生概览的一行。工作台按钮始终在最后，由用户决定要不要离开概览。
#[derive(Debug, Clone, PartialEq)]
pub enum GlanceLine {
    Status(String),
    PnlLabel(String),
    PnlAmount {
        text: String,
        tone: Tone,
    },
    PnlPercent {
        text: String,
        tone: Tone,
    },
    Metric {
        label: String,
        value: String,
    },
    Section(String),
    Todo {
        id: String,
        label: String,
        enabled: bool,
    },
    Note(String),
    Workbench {
        id: &'static str,
        label: &'static str,
    },
}

pub fn glance_lines(state: &BarState) -> Vec<GlanceLine> {
    let mut lines = Vec::new();
    match state {
        BarState::SignedOut => {
            lines.push(GlanceLine::Status("请登录".to_string()));
            lines.push(GlanceLine::Note("请在工作台中登录".to_string()));
        }
        BarState::Syncing => {
            lines.push(GlanceLine::Status("同步中".to_string()));
            lines.push(GlanceLine::Note("正在读取服务器上的今日盈亏".to_string()));
        }
        BarState::Unavailable => {
            lines.push(GlanceLine::Status("暂无数据".to_string()));
            lines.push(GlanceLine::Note("暂时没有读到今日盈亏".to_string()));
        }
        BarState::Ready {
            summary,
            tasks,
            stale,
        } => {
            if *stale {
                lines.push(GlanceLine::Note(crate::menu::STALE_NOTE.to_string()));
            }
            lines.push(GlanceLine::PnlLabel("今日盈亏".to_string()));
            lines.push(GlanceLine::PnlAmount {
                text: summary.pnl_text.clone(),
                tone: summary.tone,
            });
            lines.push(GlanceLine::PnlPercent {
                text: summary.pct_text.clone(),
                tone: summary.tone,
            });
            lines.push(GlanceLine::Metric {
                label: "净资产".to_string(),
                value: format_cny(summary.net_worth).unwrap_or_else(|| "—".to_string()),
            });
            lines.push(GlanceLine::Metric {
                label: "可投资资产".to_string(),
                value: format_cny(summary.investable).unwrap_or_else(|| "—".to_string()),
            });
            lines.push(GlanceLine::Section("待办".to_string()));
            push_tasks(&mut lines, tasks);
            if let Some(stamp) = &summary.updated_at {
                lines.push(GlanceLine::Note(format!("更新于 {stamp}")));
            }
        }
    }
    lines.push(GlanceLine::Workbench {
        id: "open",
        label: "打开工作台",
    });
    lines
}

fn push_tasks(lines: &mut Vec<GlanceLine>, tasks: &TaskSource) {
    match tasks {
        TaskSource::Engine(items) if items.is_empty() => {
            lines.push(GlanceLine::Note("没有待处理事项".to_string()));
        }
        TaskSource::Engine(items) => {
            for item in items {
                lines.push(GlanceLine::Todo {
                    id: format!("task:{}", item.key),
                    label: item.title.clone(),
                    enabled: true,
                });
            }
        }
        TaskSource::FundFallback {
            pending_purchase_count,
        } => {
            if *pending_purchase_count == 0 {
                lines.push(GlanceLine::Note("暂无待确认申购".to_string()));
            } else {
                lines.push(GlanceLine::Todo {
                    id: "task:FUND_CONFIRMATION".to_string(),
                    label: format!("{pending_purchase_count} 笔基金份额待确认"),
                    enabled: true,
                });
            }
            lines.push(GlanceLine::Note(
                "定投与账户对账待服务端更新后显示".to_string(),
            ));
        }
        TaskSource::Unavailable => {
            lines.push(GlanceLine::Note("待办暂时无法读取".to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Tone;
    use crate::summary::SummaryGlance;
    use crate::tasks::TaskSnap;

    fn summary(day_pnl: f64, tone: Tone, pnl_text: &str) -> SummaryGlance {
        SummaryGlance {
            day_pnl,
            day_pct: if day_pnl < 0.0 { -0.4 } else { 0.3 },
            net_worth: 100.0,
            investable: 80.0,
            updated_at: Some("15:04".to_string()),
            pending_purchase_count: None,
            tone,
            pnl_text: pnl_text.to_string(),
            pct_text: if day_pnl < 0.0 {
                "-0.40%".to_string()
            } else {
                "+0.30%".to_string()
            },
        }
    }

    fn task(key: &str, title: &str) -> TaskSnap {
        TaskSnap {
            key: key.to_string(),
            count: 2,
            title: title.to_string(),
            href: "/fund-confirmations".to_string(),
        }
    }

    fn assert_no_figures(lines: &[GlanceLine]) {
        let blob = format!("{lines:?}");
        assert!(!blob.contains('¥') && !blob.contains('+'), "{blob}");
        assert!(!blob.chars().any(|ch| ch.is_ascii_digit()), "{blob}");
    }

    #[test]
    fn signed_out_is_login_copy_without_numbers() {
        let lines = glance_lines(&BarState::SignedOut);
        assert_eq!(
            lines,
            vec![
                GlanceLine::Status("请登录".to_string()),
                GlanceLine::Note("请在工作台中登录".to_string()),
                GlanceLine::Workbench {
                    id: "open",
                    label: "打开工作台"
                },
            ]
        );
        assert_no_figures(&lines);
        assert_no_figures(&glance_lines(&BarState::Syncing));
        assert_no_figures(&glance_lines(&BarState::Unavailable));
        assert!(glance_lines(&BarState::Unavailable)
            .iter()
            .any(|line| matches!(line, GlanceLine::Status(text) if text == "暂无数据")));
    }

    #[test]
    fn ready_glance_colors_pnl_and_names_assets() {
        let up = glance_lines(&BarState::Ready {
            summary: summary(12.0, Tone::Up, "+12.00"),
            tasks: TaskSource::Engine(vec![task("FUND_CONFIRMATION", "2 笔基金份额待确认")]),
            stale: false,
        });
        assert!(matches!(
            &up[0],
            GlanceLine::PnlLabel(label) if label == "今日盈亏"
        ));
        assert!(matches!(
            &up[1],
            GlanceLine::PnlAmount { text, tone } if text == "+12.00" && *tone == Tone::Up
        ));
        assert!(matches!(
            &up[2],
            GlanceLine::PnlPercent { text, tone } if text == "+0.30%" && *tone == Tone::Up
        ));
        assert!(up.iter().any(|line| matches!(line, GlanceLine::Metric { label, value } if label == "净资产" && value == "¥100.00")));
        assert!(up.iter().any(|line| matches!(line, GlanceLine::Metric { label, value } if label == "可投资资产" && value == "¥80.00")));
        assert!(up.iter().any(|line| matches!(line, GlanceLine::Todo { id, label, enabled: true } if id == "task:FUND_CONFIRMATION" && label == "2 笔基金份额待确认")));
        assert!(up
            .iter()
            .any(|line| matches!(line, GlanceLine::Note(text) if text == "更新于 15:04")));
        assert!(matches!(
            up.last(),
            Some(GlanceLine::Workbench {
                id: "open",
                label: "打开工作台"
            })
        ));

        let down = glance_lines(&BarState::Ready {
            summary: summary(-1.2, Tone::Down, "-1.20"),
            tasks: TaskSource::Unavailable,
            stale: false,
        });
        assert!(matches!(
            &down[1],
            GlanceLine::PnlAmount { text, tone } if text == "-1.20" && *tone == Tone::Down
        ));
        assert!(down
            .iter()
            .any(|line| matches!(line, GlanceLine::Note(text) if text == "待办暂时无法读取")));
        assert!(!format!("{down:?}").contains("定投") && !format!("{down:?}").contains("0 笔"));
    }

    #[test]
    fn missing_task_types_are_not_rendered_as_zero() {
        let fallback = glance_lines(&BarState::Ready {
            summary: summary(12.0, Tone::Up, "+12.00"),
            tasks: TaskSource::FundFallback {
                pending_purchase_count: 2,
            },
            stale: false,
        });
        let blob = format!("{fallback:?}");
        assert!(blob.contains("2 笔基金份额待确认"));
        assert!(blob.contains("定投与账户对账待服务端更新后显示"));
        assert!(!blob.contains("定投节点") && !blob.contains("账户本月") && !blob.contains("0 笔"));

        let none = glance_lines(&BarState::Ready {
            summary: summary(12.0, Tone::Up, "+12.00"),
            tasks: TaskSource::FundFallback {
                pending_purchase_count: 0,
            },
            stale: false,
        });
        let blob = format!("{none:?}");
        assert!(blob.contains("暂无待确认申购"));
        assert!(blob.contains("待服务端更新后显示"));
        assert!(!blob.contains("0 笔") && !blob.contains("定投节点"));

        let empty = glance_lines(&BarState::Ready {
            summary: summary(12.0, Tone::Up, "+12.00"),
            tasks: TaskSource::Engine(vec![]),
            stale: false,
        });
        assert!(empty
            .iter()
            .any(|line| matches!(line, GlanceLine::Note(text) if text == "没有待处理事项")));
        assert!(!empty
            .iter()
            .any(|line| matches!(line, GlanceLine::Todo { .. })));
    }

    #[test]
    fn stale_glance_keeps_amounts_and_marks_them_old() {
        let lines = glance_lines(&BarState::Ready {
            summary: summary(12.0, Tone::Up, "+12.00"),
            tasks: TaskSource::Unavailable,
            stale: true,
        });
        assert!(lines
            .iter()
            .any(|line| matches!(line, GlanceLine::PnlAmount { text, .. } if text == "+12.00")));
        assert!(lines.iter().any(|line| matches!(line, GlanceLine::Metric { label, value } if label == "净资产" && value == "¥100.00")));
        assert!(lines
            .iter()
            .any(|line| matches!(line, GlanceLine::Note(text) if text == crate::menu::STALE_NOTE)));
    }
}
