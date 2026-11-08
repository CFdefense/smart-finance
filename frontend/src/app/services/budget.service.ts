import { inject, Injectable } from '@angular/core';
import { HttpClient, HttpParams } from '@angular/common/http';
import { Observable } from 'rxjs';

/**
 * Spending categories supported by the Rust backend.
 *
 * These values must match the Category enum in src/models/category.rs.
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
 * Information required to create a recurring monthly budget.
 */
export interface CreateBudgetRequest {
  name: string;
  category: BudgetCategory;
  monthly_limit: number;
  active?: boolean;
}

/**
 * Fields the user can change after creating a budget.
 *
 * The category is intentionally excluded because the backend
 * does not allow changing it after creation.
 */
export interface UpdateBudgetRequest {
  name?: string;
  monthly_limit?: number;
  active?: boolean;
}

/**
 * Budget data returned by the backend, including spending
 * calculated from synced Plaid transactions.
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
 * Communicates with the Rust budget endpoints.
 *
 * All requests include the user's authentication cookie.
 */
@Injectable({
  providedIn: 'root',
})
export class BudgetService {
  private readonly http = inject(HttpClient);

  private readonly baseUrl = 'http://localhost:3001/api/budgets';

  /**
   * Returns the user's budgets and their spending.
   *
   * An optional month can be supplied in YYYY-MM format.
   * Without one, the backend uses the current month.
   */
  getBudgets(month?: string): Observable<BudgetsResponse> {
    const params = month ? new HttpParams().set('month', month) : new HttpParams();

    return this.http.get<BudgetsResponse>(this.baseUrl, {
      params,
      withCredentials: true,
    });
  }

  /**
   * Retrieves a single budget by its ID.
   */
  getBudget(id: number, month?: string): Observable<Budget> {
    const params = month ? new HttpParams().set('month', month) : new HttpParams();

    return this.http.get<Budget>(`${this.baseUrl}/${id}`, {
      params,
      withCredentials: true,
    });
  }

  /**
   * Creates a monthly spending budget.
   *
   * The backend returns HTTP 201 with no response body.
   */
  createBudget(request: CreateBudgetRequest): Observable<void> {
    return this.http.post<void>(this.baseUrl, request, {
      withCredentials: true,
    });
  }

  /**
   * Updates an existing budget's name, limit, or active state.
   */
  updateBudget(id: number, changes: UpdateBudgetRequest): Observable<Budget> {
    return this.http.put<Budget>(`${this.baseUrl}/${id}`, changes, { withCredentials: true });
  }

  /**
   * Permanently deletes a budget.
   *
   * The backend returns HTTP 204 with no response body.
   */
  deleteBudget(id: number): Observable<void> {
    return this.http.delete<void>(`${this.baseUrl}/${id}`, {
      withCredentials: true,
    });
  }
}
