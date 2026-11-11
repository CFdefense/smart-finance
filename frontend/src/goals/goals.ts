import { HttpClient } from '@angular/common/http';
import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { finalize, Observable } from 'rxjs';

import { Goal, GoalsService, GoalStatus } from '../app/services/goals.service';
import { API_BASE_URL } from '../app/api-config';

/** Only the Plaid fields needed to choose an eligible checking/savings account. */
interface GoalAccount {
  account_id: string;
  name: string;
  account_type: string;
  account_subtype: string | null;
  mask: string | null;
  current_balance: number | null;
}

interface AccountsResponse {
  accounts: GoalAccount[];
}

/** The backend supports only account-balance goals linked to depository accounts. */
@Component({
  selector: 'app-goals',
  imports: [FormsModule],
  templateUrl: './goals.html',
  styleUrl: './goals.css',
})
export class GoalsPage implements OnInit {
  private readonly goalsService = inject(GoalsService);
  private readonly http = inject(HttpClient);
  private readonly router = inject(Router);

  readonly goals = signal<Goal[]>([]);
  readonly accounts = signal<GoalAccount[]>([]);
  readonly loading = signal(false);
  readonly accountsLoading = signal(false);
  readonly saving = signal(false);
  readonly workingId = signal<number | null>(null);
  readonly errorMessage = signal('');
  readonly accountError = signal('');
  readonly successMessage = signal('');
  readonly showForm = signal(false);
  readonly editingId = signal<number | null>(null);

  // Plain properties work naturally with Angular's [(ngModel)] form controls.
  name = '';
  description = '';
  targetAmount: number | null = null;
  targetDate = '';
  accountId = '';
  statusFilter: 'ALL' | GoalStatus = 'ALL';

  ngOnInit(): void {
    this.loadGoals();
    this.loadAccounts();
  }

  /** The backend calculates percentages and remaining amounts; don't duplicate that math. */
  loadGoals(): void {
    this.loading.set(true);
    this.errorMessage.set('');
    this.goalsService
      .getGoals()
      .pipe(finalize(() => this.loading.set(false)))
      .subscribe({
        next: (response) => this.goals.set(response.goals),
        error: () => this.errorMessage.set('Unable to load goals. Please try again.'),
      });
  }

  /**
   * Get available Plaid accounts directly from the existing endpoint.
   * Once PR #73 lands, this can be replaced with PlaidService.getAccounts().
   */
  loadAccounts(): void {
    this.accountsLoading.set(true);
    this.accountError.set('');
    this.http
      .get<AccountsResponse>(`${API_BASE_URL}/plaid/accounts`, {
        withCredentials: true,
      })
      .pipe(finalize(() => this.accountsLoading.set(false)))
      .subscribe({
        next: (response) => {
          this.accounts.set(
            response.accounts.filter((account) => account.account_type === 'depository'),
          );
        },
        error: () => this.accountError.set('Unable to load connected bank accounts.'),
      });
  }

  /** Open a blank form for creating a new goal. */
  startCreate(): void {
    this.clearForm();
    this.errorMessage.set('');
    this.successMessage.set('');
    this.showForm.set(true);
  }

  /** Populate the form with an existing goal's editable fields. */
  startEdit(goal: Goal): void {
    this.clearForm();
    this.editingId.set(goal.id);
    this.name = goal.name;
    this.description = goal.description ?? '';
    this.targetAmount = goal.target_amount;
    this.targetDate = goal.target_date ?? '';
    this.accountId = goal.account_id;
    this.errorMessage.set('');
    this.successMessage.set('');
    this.showForm.set(true);
  }

  cancelForm(): void {
    this.showForm.set(false);
    this.clearForm();
  }

  /**
   * Save creates or edits a goal. The API returns no body on create, so reload
   * the goals list after success to obtain the new ID and server-derived progress.
   */
  saveGoal(): void {
    if (this.saving()) return;
    this.errorMessage.set('');
    this.successMessage.set('');
    const name = this.name.trim();
    const amount = this.targetAmount;
    if (!name || amount === null || !Number.isFinite(amount) || amount <= 0) {
      this.errorMessage.set('Enter a goal name and a positive target amount.');
      return;
    }
    if (!this.accountId) {
      this.errorMessage.set('Select a checking or savings account.');
      return;
    }
    if (this.targetDate && this.targetDate <= this.localDateToday()) {
      this.errorMessage.set('The target date must be in the future.');
      return;
    }

    this.saving.set(true);
    const description = this.description.trim() || null;
    const target_date = this.targetDate || null;
    const id = this.editingId();
    const request: Observable<Goal | void> =
      id === null
        ? this.goalsService.createGoal({
            name,
            description,
            target_amount: amount,
            target_date,
            account_id: this.accountId,
          })
        : this.goalsService.updateGoal(id, {
            name,
            description,
            target_amount: amount,
            target_date,
            account_id: this.accountId,
          });

    request.pipe(finalize(() => this.saving.set(false))).subscribe({
      next: () => {
        this.cancelForm();
        this.successMessage.set(
          id === null ? 'Goal created successfully!' : 'Goal updated successfully!',
        );
        this.loadGoals();
      },
      error: () =>
        this.errorMessage.set(
          'Unable to save goal. Make sure the target is above the account starting balance and try again.',
        ),
    });
  }

  /** Archive and restore use the update endpoint; deleting is permanent. */
  toggleArchived(goal: Goal): void {
    if (this.workingId() !== null) return;
    this.errorMessage.set('');
    this.successMessage.set('');
    this.workingId.set(goal.id);
    this.goalsService
      .updateGoal(goal.id, { archived: goal.status !== 'ARCHIVED' })
      .pipe(finalize(() => this.workingId.set(null)))
      .subscribe({
        next: () => {
          this.successMessage.set(goal.status === 'ARCHIVED' ? 'Goal restored.' : 'Goal archived.');
          this.loadGoals();
        },
        error: () => this.errorMessage.set('Unable to update goal status.'),
      });
  }

  /** Ask for confirmation before an irreversible delete. */
  deleteGoal(goal: Goal): void {
    if (this.workingId() !== null || !window.confirm(`Delete goal "${goal.name}" permanently?`))
      return;
    this.errorMessage.set('');
    this.successMessage.set('');
    this.workingId.set(goal.id);
    this.goalsService
      .deleteGoal(goal.id)
      .pipe(finalize(() => this.workingId.set(null)))
      .subscribe({
        next: () => {
          this.successMessage.set('Goal deleted.');
          this.loadGoals();
        },
        error: () => this.errorMessage.set('Unable to delete goal.'),
      });
  }

  /** Keep status filtering local so changing tabs does not trigger unnecessary requests. */
  filteredGoals(): Goal[] {
    return this.statusFilter === 'ALL'
      ? this.goals()
      : this.goals().filter((goal) => goal.status === this.statusFilter);
  }

  accountLabel(accountId: string): string {
    const account = this.accounts().find((item) => item.account_id === accountId);
    return account
      ? `${account.name}${account.mask ? ` •••• ${account.mask}` : ''}`
      : 'Connected account';
  }

  money(value: number): string {
    return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(value);
  }

  backToDashboard(): void {
    this.router.navigate(['/dashboard']);
  }

  private clearForm(): void {
    this.editingId.set(null);
    this.name = '';
    this.description = '';
    this.targetAmount = null;
    this.targetDate = '';
    this.accountId = '';
  }

  private localDateToday(): string {
    const today = new Date();
    const year = today.getFullYear();
    const month = String(today.getMonth() + 1).padStart(2, '0');
    const day = String(today.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }
}
