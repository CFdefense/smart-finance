import { HttpClient, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { API_BASE_URL } from '../api-config';

import type {
  GoalStatus,
  Goal,
  GoalsResponse,
  CreateGoalRequest,
  UpdateGoalRequest,
} from '../models/goals.models';

export type {
  GoalStatus,
  Goal,
  GoalsResponse,
  CreateGoalRequest,
  UpdateGoalRequest,
} from '../models/goals.models';

/**
 * Connects Angular Goals components to the existing authenticated Rust API.
 * The backend calculates goal progress and validates ownership, amounts, and dates.
 */
@Injectable({ providedIn: 'root' })
export class GoalsService {
  private readonly http = inject(HttpClient);
  private readonly baseUrl = `${API_BASE_URL}/goals`;
  private readonly options = { withCredentials: true };

  /** Fetch goals, optionally filtered by their computed status. */
  getGoals(status?: GoalStatus): Observable<GoalsResponse> {
    const params = status ? new HttpParams().set('status', status) : new HttpParams();
    return this.http.get<GoalsResponse>(this.baseUrl, {
      ...this.options,
      params,
    });
  }

  /** Fetch one goal with its latest balance-derived progress. */
  getGoal(id: number): Observable<Goal> {
    return this.http.get<Goal>(`${this.baseUrl}/${id}`, this.options);
  }

  /** Create a goal. The API returns HTTP 201 without a response body. */
  createGoal(request: CreateGoalRequest): Observable<void> {
    return this.http.post<void>(this.baseUrl, request, this.options);
  }

  /** Update provided fields, including archiving or unarchiving a goal. */
  updateGoal(id: number, changes: UpdateGoalRequest): Observable<Goal> {
    return this.http.put<Goal>(`${this.baseUrl}/${id}`, changes, this.options);
  }

  /** Permanently delete a goal. The API returns HTTP 204 without a body. */
  deleteGoal(id: number): Observable<void> {
    return this.http.delete<void>(`${this.baseUrl}/${id}`, this.options);
  }
}
