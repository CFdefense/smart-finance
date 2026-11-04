import { Component, inject, OnInit, signal } from '@angular/core';
import { CurrencyPipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { HttpErrorResponse } from '@angular/common/http';

import {
  BudgetService,
  type Budget,
  type BudgetCategory,
  type CreateBudgetRequest,
} from '../app/services/budget.service';

/**
 * Categories accepted by the budget API.
 *
 * The value is sent to Rust, while the label is displayed
 * to the user.
 */
const CATEGORIES: { value: BudgetCategory; label: string }[] = [
  { value: 'SHOPPING', label: 'Shopping' },
  { value: 'ENTERTAINMENT', label: 'Entertainment' },
  { value: 'UTILITIES', label: 'Utilities' },
  { value: 'HEALTHCARE', label: 'Healthcare' },
  { value: 'FOOD_DINING', label: 'Food & Dining' },
  { value: 'GROCERIES', label: 'Groceries' },
  { value: 'TRANSPORTATION', label: 'Transportation' },
  { value: 'OTHER', label: 'Other' },
];

/**
 * Manage monthly spending budgets.
 *
 * This is a reusable component. We can display it on the
 * dashboard or another page without changing its API logic.
 */
@Component({
  selector: 'app-budgets',
  imports: [FormsModule, CurrencyPipe],
  templateUrl: './budgets.html',
  styleUrl: './budgets.css',
})
export class Budgets implements OnInit {
  private readonly budgetService = inject(BudgetService);

  readonly categories = CATEGORIES;

  /**
   * Converts a budget's API category into a friendly
   * display label.
   *
   * Example: FOOD_DINING becomes Food & Dining.
   */
  categoryLabel(category: BudgetCategory): string {
    return this.categories.find((option) => option.value === category)?.label ?? category;
  }

  readonly budgets = signal<Budget[]>([]);
  readonly loading = signal(false);
  readonly saving = signal(false);
  readonly deletingId = signal<number | null>(null);

  readonly errorMessage = signal('');
  readonly successMessage = signal('');

  /**
   * Determines whether the create/edit form is visible.
   */
  readonly showForm = signal(false);

  /**
   * Null means create mode; otherwise we are editing
   * the budget with this ID.
   */
  readonly editingId = signal<number | null>(null);

  /**
   * Month options for the previous 12 months, current month,
   * and next 6 months.
   *
   * The backend expects YYYY-MM, while the dropdown shows
   * friendly labels such as "November 2026".
   */
  readonly monthOptions = Array.from({ length: 19 }, (_, index) => {
    const today = new Date();

    // The first option is six months in the future.
    const date = new Date(today.getFullYear(), today.getMonth() + 6 - index, 1);

    const value = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}`;

    const label = new Intl.DateTimeFormat('en-US', {
      month: 'long',
      year: 'numeric',
    }).format(date);

    return { value, label };
  });

  /**
   * Initially show the current month, not the first
   * option in the dropdown.
   */
  selectedMonth = (() => {
    const today = new Date();

    return `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}`;
  })();

  /**
   * Returns the selected month's friendly display name.
   */
  get selectedMonthLabel(): string {
    return (
      this.monthOptions.find((month) => month.value === this.selectedMonth)?.label ??
      this.selectedMonth
    );
  }

  /**
   * Update the selected month before requesting new spending data.
   */
  changeMonth(month: string): void {
    this.selectedMonth = month;
    this.loadBudgets();
  }

  /**
   * Two-way bound form fields.
   */
  formName = '';
  formCategory: BudgetCategory = 'GROCERIES';
  formMonthlyLimit: number | null = null;
  formActive = true;

  ngOnInit(): void {
    this.loadBudgets();
  }

  /**
   * Retrieves budgets and spending for the selected month.
   */
  loadBudgets(): void {
    this.loading.set(true);
    this.errorMessage.set('');

    this.budgetService.getBudgets(this.selectedMonth).subscribe({
      next: (response) => {
        this.budgets.set(response.budgets);
        this.loading.set(false);
      },
      error: () => {
        this.errorMessage.set('Unable to load budgets. Please try again.');
        this.loading.set(false);
      },
    });
  }

  /**
   * Returns the combined monthly limit of active budgets.
   */
  get totalBudget(): number {
    return this.budgets()
      .filter((budget) => budget.active)
      .reduce((sum, budget) => sum + budget.monthly_limit, 0);
  }

  /**
   * Returns spending across all active budget categories.
   */
  get totalSpent(): number {
    return this.budgets()
      .filter((budget) => budget.active)
      .reduce((sum, budget) => sum + budget.spent, 0);
  }

  /**
   * Begins creating a new budget.
   */
  startCreate(): void {
    this.resetForm();
    this.showForm.set(true);
    this.errorMessage.set('');
    this.successMessage.set('');
  }

  /**
   * Loads a budget's existing values into the edit form.
   */
  startEdit(budget: Budget): void {
    this.editingId.set(budget.id);

    this.formName = budget.name;
    this.formCategory = budget.category;
    this.formMonthlyLimit = budget.monthly_limit;
    this.formActive = budget.active;

    this.showForm.set(true);
    this.errorMessage.set('');
    this.successMessage.set('');
  }

  /**
   * Closes the form without saving.
   */
  cancelForm(): void {
    this.resetForm();
    this.showForm.set(false);
  }

  /**
   * Creates a new budget or updates an existing one.
   */
  saveBudget(): void {
    const name = this.formName.trim();
    const limit = Number(this.formMonthlyLimit);

    if (!name) {
      this.errorMessage.set('Please enter a budget name.');
      return;
    }

    if (this.formMonthlyLimit === null || !Number.isFinite(limit) || limit <= 0) {
      this.errorMessage.set('Please enter a positive monthly limit.');
      return;
    }

    this.saving.set(true);
    this.errorMessage.set('');
    this.successMessage.set('');

    const editingId = this.editingId();

    if (editingId !== null) {
      this.budgetService
        .updateBudget(editingId, {
          name,
          monthly_limit: limit,
          active: this.formActive,
        })
        .subscribe({
          next: () => this.onSaveSuccess('Budget updated successfully!'),
          error: (error: HttpErrorResponse) => this.onSaveError(error),
        });

      return;
    }

    const request: CreateBudgetRequest = {
      name,
      category: this.formCategory,
      monthly_limit: limit,
      active: this.formActive,
    };

    this.budgetService.createBudget(request).subscribe({
      next: () => this.onSaveSuccess('Budget created successfully!'),
      error: (error: HttpErrorResponse) => this.onSaveError(error),
    });
  }

  /**
   * Deletes a budget after confirmation.
   */
  deleteBudget(budget: Budget): void {
    if (!window.confirm(`Delete the "${budget.name}" budget?`)) {
      return;
    }

    this.deletingId.set(budget.id);
    this.errorMessage.set('');
    this.successMessage.set('');

    this.budgetService.deleteBudget(budget.id).subscribe({
      next: () => {
        this.deletingId.set(null);
        this.successMessage.set('Budget deleted successfully!');
        this.loadBudgets();
      },
      error: () => {
        this.deletingId.set(null);
        this.errorMessage.set('Unable to delete this budget.');
      },
    });
  }

  /**
   * Called after the API successfully creates or updates a budget.
   */
  private onSaveSuccess(message: string): void {
    this.saving.set(false);
    this.cancelForm();
    this.successMessage.set(message);
    this.loadBudgets();
  }

  /**
   * Provides an understandable error for failed saves.
   *
   * HTTP 409 occurs when the user already has a budget
   * for the selected category.
   */
  private onSaveError(error: HttpErrorResponse): void {
    this.saving.set(false);

    if (error.status === 409) {
      this.errorMessage.set('You already have a budget for this category.');
      return;
    }

    this.errorMessage.set('Unable to save the budget. Please try again.');
  }

  /**
   * Clears all editable fields.
   */
  private resetForm(): void {
    this.editingId.set(null);
    this.formName = '';
    this.formCategory = 'GROCERIES';
    this.formMonthlyLimit = null;
    this.formActive = true;
  }

  /**
   * Clamps a progress percentage to the range supported
   * by the HTML progress element.
   *
   * We still display the actual percentage separately,
   * including values greater than 100% for overspending.
   */
  progressValue(percent: number): number {
    return Math.max(0, Math.min(percent, 100));
  }
}
