//! Spending categories shared by transactions, budgets and rules.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A spending category (matches the `CategoryType` enumeration in the class diagram).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Category {
    /// General retail purchases.
    Shopping,
    /// Entertainment and recreation.
    Entertainment,
    /// Utility bills, rent-adjacent services, telecom.
    Utilities,
    /// Medical and personal care.
    Healthcare,
    /// Restaurants, coffee shops, bars.
    FoodDining,
    /// Supermarkets and groceries.
    Groceries,
    /// Transit, fuel, rideshare, travel.
    Transportation,
    /// Anything that does not fit another category.
    Other,
}

impl Category {
    /// The database / wire representation of this category.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Shopping => "SHOPPING",
            Category::Entertainment => "ENTERTAINMENT",
            Category::Utilities => "UTILITIES",
            Category::Healthcare => "HEALTHCARE",
            Category::FoodDining => "FOOD_DINING",
            Category::Groceries => "GROCERIES",
            Category::Transportation => "TRANSPORTATION",
            Category::Other => "OTHER",
        }
    }

    /// Parses the wire representation (case-sensitive, e.g. `FOOD_DINING`).
    pub fn parse(value: &str) -> Option<Category> {
        match value {
            "SHOPPING" => Some(Category::Shopping),
            "ENTERTAINMENT" => Some(Category::Entertainment),
            "UTILITIES" => Some(Category::Utilities),
            "HEALTHCARE" => Some(Category::Healthcare),
            "FOOD_DINING" => Some(Category::FoodDining),
            "GROCERIES" => Some(Category::Groceries),
            "TRANSPORTATION" => Some(Category::Transportation),
            "OTHER" => Some(Category::Other),
            _ => None,
        }
    }

    /// Maps Plaid's `personal_finance_category` (`primary` + optional `detailed`) onto a [`Category`].
    ///
    /// Groceries are only distinguishable from other food in the `detailed` value.
    /// Anything unmapped falls back to [`Category::Other`].
    pub fn from_plaid(primary: &str, detailed: Option<&str>) -> Category {
        if detailed.is_some_and(|d| d.starts_with("FOOD_AND_DRINK_GROCERIES")) {
            return Category::Groceries;
        }
        match primary {
            "FOOD_AND_DRINK" => Category::FoodDining,
            "GENERAL_MERCHANDISE" | "HOME_IMPROVEMENT" => Category::Shopping,
            "ENTERTAINMENT" => Category::Entertainment,
            "RENT_AND_UTILITIES" => Category::Utilities,
            "MEDICAL" | "PERSONAL_CARE" => Category::Healthcare,
            "TRANSPORTATION" | "TRAVEL" => Category::Transportation,
            _ => Category::Other,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const ALL: [Category; 8] = [
        Category::Shopping,
        Category::Entertainment,
        Category::Utilities,
        Category::Healthcare,
        Category::FoodDining,
        Category::Groceries,
        Category::Transportation,
        Category::Other,
    ];

    #[test]
    fn parse_round_trips_every_category() {
        for c in ALL {
            assert_eq!(Category::parse(c.as_str()), Some(c));
        }
    }

    #[test]
    fn parse_rejects_unknown_and_wrong_case() {
        assert_eq!(Category::parse("food_dining"), None);
        assert_eq!(Category::parse("NOPE"), None);
        assert_eq!(Category::parse(""), None);
    }

    #[test]
    fn serde_uses_screaming_snake_case() {
        assert_eq!(
            serde_json::to_string(&Category::FoodDining).unwrap(),
            "\"FOOD_DINING\""
        );
        let c: Category = serde_json::from_str("\"GROCERIES\"").unwrap();
        assert_eq!(c, Category::Groceries);
    }

    #[test]
    fn from_plaid_maps_primary_categories() {
        assert_eq!(
            Category::from_plaid("FOOD_AND_DRINK", None),
            Category::FoodDining
        );
        assert_eq!(
            Category::from_plaid("GENERAL_MERCHANDISE", None),
            Category::Shopping
        );
        assert_eq!(
            Category::from_plaid("ENTERTAINMENT", None),
            Category::Entertainment
        );
        assert_eq!(
            Category::from_plaid("RENT_AND_UTILITIES", None),
            Category::Utilities
        );
        assert_eq!(Category::from_plaid("MEDICAL", None), Category::Healthcare);
        assert_eq!(
            Category::from_plaid("TRAVEL", None),
            Category::Transportation
        );
    }

    #[test]
    fn from_plaid_detects_groceries_from_detailed() {
        assert_eq!(
            Category::from_plaid("FOOD_AND_DRINK", Some("FOOD_AND_DRINK_GROCERIES")),
            Category::Groceries
        );
        assert_eq!(
            Category::from_plaid("FOOD_AND_DRINK", Some("FOOD_AND_DRINK_COFFEE")),
            Category::FoodDining
        );
    }

    #[test]
    fn from_plaid_unknown_falls_back_to_other() {
        assert_eq!(Category::from_plaid("INCOME", None), Category::Other);
        assert_eq!(Category::from_plaid("", None), Category::Other);
    }
}
