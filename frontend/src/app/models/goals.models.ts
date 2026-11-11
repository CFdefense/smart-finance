/** Shared goals API data models. */

/** A goal's status, computed by the Rust backend from the linked account balance. */
export type GoalStatus = 'ACTIVE' | 'COMPLETED' | 'ARCHIVED';

/** A saved goal, including the current balance and progress from the backend. */
export interface Goal {
  id: number;
  name: string;
  description: string | null;
  target_amount: number;
  target_date: string | null;
  account_id: string;
  baseline_balance: number;
  current_balance: number;
  progress_percent: number;
  remaining: number;
  on_track: boolean | null;
  status: GoalStatus;
  created_at: string;
}

/** The collection returned by GET /api/goals. */
export interface GoalsResponse {
  goals: Goal[];
}

/** Data required to create a goal. Plaid account ownership is checked by Rust. */
export interface CreateGoalRequest {
  name: string;
  target_amount: number;
  account_id: string;
  description?: string | null;
  target_date?: string | null;
  baseline_balance?: number;
}

/**
 * Fields that can be changed on an existing goal.
 * A null description or date clears that field; undefined leaves it unchanged.
 * If baseline_balance is supplied, account_id must also be supplied.
 */
export interface UpdateGoalRequest {
  name?: string;
  description?: string | null;
  target_amount?: number;
  target_date?: string | null;
  account_id?: string;
  baseline_balance?: number;
  archived?: boolean;
}
