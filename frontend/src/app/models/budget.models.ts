/**
 * Data models for the budgeting feature.
 *
 * These types describe the JSON exchanged between
 * the Angular frontend and the Rust backend.
 *
 * No HTTP requests or Angular service logic belong here.
 */

/**
 * Spending categories supported by the Rust backend.
 *
 * These must match the backend's Category enum.
 */
export type BudgetCategory =
  | 'SHOPPING'
  | 'ENTERTAINMENT'
  | 'UTILITIES'
  | 'HEALTHCARE'
  | 'FOOD_DINING'
  | 'GROCERIES'
  | 'TRANSPORTATION'
  | 'OTHER';

/**
 * Request body used to create a monthly budget.
 */
export interface CreateBudgetRequest {
  name: string;
  category: BudgetCategory;
  monthly_limit: number;
  active?: boolean;
}

/**
 * Fields that can be changed on an existing budget.
 *
 * Category is excluded because the backend does not
 * currently allow changing it after creation.
 */
export interface UpdateBudgetRequest {
  name?: string;
  monthly_limit?: number;
  active?: boolean;
}

/**
 * Budget information returned by the backend.
 *
 * Spending and progress values are calculated using
 * the user's synchronized financial transactions.
 */
export interface Budget {
  id: number;
  name: string;
  category: BudgetCategory;
  monthly_limit: number;
  active: boolean;
  month: string;
  spent: number;
  remaining: number;
  percent_used: number;
  over_budget: boolean;
}

/**
 * Response from GET /api/budgets.
 */
export interface BudgetsResponse {
  month: string;
  budgets: Budget[];
}

/**
 * Total spending in one category during a month.
 */
export interface CategorySpending {
  category: BudgetCategory;
  amount: number;
}

/**
 * Response from GET /api/budgets/spending-by-category.
 *
 * Includes categories even when the user has not
 * created a monthly budget for them.
 */
export interface CategorySpendingResponse {
  month: string;
  total: number;
  categories: CategorySpending[];
}
