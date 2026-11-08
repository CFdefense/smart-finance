import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { HttpErrorResponse } from '@angular/common/http';
import { of, throwError } from 'rxjs';
import { ComponentFixture } from '@angular/core/testing';
import { Subject } from 'rxjs';

import { Budgets } from './budgets';
import { BudgetService, type Budget } from '../app/services/budget.service';

describe('Budgets', () => {
  let component: Budgets;
  let fixture: ComponentFixture<Budgets>;

  let budgetService: {
    getBudgets: ReturnType<typeof vi.fn>;
    createBudget: ReturnType<typeof vi.fn>;
    updateBudget: ReturnType<typeof vi.fn>;
    deleteBudget: ReturnType<typeof vi.fn>;
  };

  const groceryBudget: Budget = {
    id: 1,
    name: 'Groceries',
    category: 'GROCERIES',
    monthly_limit: 400,
    active: true,
    month: '2026-11',
    spent: 150,
    remaining: 250,
    percent_used: 37.5,
    over_budget: false,
  };

  beforeEach(async () => {
    budgetService = {
      getBudgets: vi.fn().mockReturnValue(of({ month: '2026-11', budgets: [groceryBudget] })),
      createBudget: vi.fn().mockReturnValue(of(null)),
      updateBudget: vi.fn().mockReturnValue(of(groceryBudget)),
      deleteBudget: vi.fn().mockReturnValue(of(null)),
    };

    await TestBed.configureTestingModule({
      imports: [Budgets],
      providers: [{ provide: BudgetService, useValue: budgetService }],
    }).compileComponents();

    fixture = TestBed.createComponent(Budgets);
    component = fixture.componentInstance;
  });

  it('loads budgets when initialized', () => {
    component.ngOnInit();

    expect(budgetService.getBudgets).toHaveBeenCalled();
    expect(component.budgets()).toEqual([groceryBudget]);
    expect(component.loading()).toBe(false);
  });

  it('handles failure to load budgets', () => {
    budgetService.getBudgets.mockReturnValue(throwError(() => new Error('Network error')));

    component.loadBudgets();

    expect(component.errorMessage()).toBe('Unable to load budgets. Please try again.');
    expect(component.loading()).toBe(false);
  });

  it('calculates budget totals using active budgets only', () => {
    const inactiveBudget: Budget = {
      ...groceryBudget,
      id: 2,
      name: 'Shopping',
      category: 'SHOPPING',
      monthly_limit: 500,
      spent: 200,
      active: false,
    };

    component.budgets.set([groceryBudget, inactiveBudget]);

    expect(component.totalBudget).toBe(400);
    expect(component.totalSpent).toBe(150);
  });

  it('starts creating a budget with a clean form', () => {
    component.startCreate();

    expect(component.showForm()).toBe(true);
    expect(component.editingId()).toBe(null);
    expect(component.formName).toBe('');
    expect(component.formCategory).toBe('GROCERIES');
  });

  it('populates the form when editing a budget', () => {
    component.startEdit(groceryBudget);

    expect(component.showForm()).toBe(true);
    expect(component.editingId()).toBe(1);
    expect(component.formName).toBe('Groceries');
    expect(component.formMonthlyLimit).toBe(400);
    expect(component.formActive).toBe(true);
  });

  it('rejects a missing budget name', () => {
    component.formName = '';
    component.formMonthlyLimit = 400;

    component.saveBudget();

    expect(component.errorMessage()).toBe('Please enter a budget name.');
    expect(budgetService.createBudget).not.toHaveBeenCalled();
  });

  it('rejects an invalid monthly limit', () => {
    component.formName = 'Groceries';
    component.formMonthlyLimit = 0;

    component.saveBudget();

    expect(component.errorMessage()).toBe('Please enter a positive monthly limit.');
    expect(budgetService.createBudget).not.toHaveBeenCalled();
  });

  it('creates a valid budget and refreshes the list', () => {
    component.startCreate();
    component.formName = 'Groceries';
    component.formMonthlyLimit = 400;

    component.saveBudget();

    expect(budgetService.createBudget).toHaveBeenCalledWith({
      name: 'Groceries',
      category: 'GROCERIES',
      monthly_limit: 400,
      active: true,
    });

    expect(component.successMessage()).toBe('Budget created successfully!');
    expect(component.showForm()).toBe(false);
    expect(budgetService.getBudgets).toHaveBeenCalled();
    expect(component.saving()).toBe(false);
  });

  it('updates an existing budget', () => {
    component.startEdit(groceryBudget);
    component.formMonthlyLimit = 500;

    component.saveBudget();

    expect(budgetService.updateBudget).toHaveBeenCalledWith(
      1,
      expect.objectContaining({
        monthly_limit: 500,
      }),
    );

    expect(component.successMessage()).toBe('Budget updated successfully!');
  });

  it('handles a duplicate budget category', () => {
    budgetService.createBudget.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 409 })),
    );

    component.formName = 'Duplicate';
    component.formMonthlyLimit = 300;

    component.saveBudget();

    expect(component.errorMessage()).toBe('You already have a budget for this category.');
    expect(component.saving()).toBe(false);
  });

  it('handles a general error when saving', () => {
    budgetService.createBudget.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 500 })),
    );

    component.formName = 'Groceries';
    component.formMonthlyLimit = 400;

    component.saveBudget();

    expect(component.errorMessage()).toBe('Unable to save the budget. Please try again.');
    expect(component.saving()).toBe(false);
  });

  it('does not delete a budget when confirmation is canceled', () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);

    component.deleteBudget(groceryBudget);

    expect(budgetService.deleteBudget).not.toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('deletes a confirmed budget and refreshes the list', () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    component.deleteBudget(groceryBudget);

    expect(budgetService.deleteBudget).toHaveBeenCalledWith(1);
    expect(component.deletingId()).toBe(null);
    expect(component.successMessage()).toBe('Budget deleted successfully!');

    confirmSpy.mockRestore();
  });

  it('preserves the list when deleting fails', () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    budgetService.deleteBudget.mockReturnValue(throwError(() => new Error('Delete failed')));

    component.budgets.set([groceryBudget]);
    component.deleteBudget(groceryBudget);

    expect(component.budgets()).toEqual([groceryBudget]);
    expect(component.errorMessage()).toBe('Unable to delete this budget.');
    expect(component.deletingId()).toBe(null);

    confirmSpy.mockRestore();
  });

  it('clamps progress to the HTML progress range', () => {
    expect(component.progressValue(-10)).toBe(0);
    expect(component.progressValue(50)).toBe(50);
    expect(component.progressValue(125)).toBe(100);
  });

  /**
   * Verifies the real Angular template renders budgets
   * returned by the API.
   */
  it('renders existing budgets in the interface', () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    expect(html.textContent).toContain('Manage Budgets');
    expect(html.textContent).toContain('Groceries');
    expect(html.textContent).toContain('$150.00');
    expect(html.querySelector('progress')).not.toBeNull();
  });

  /**
   * A new user should see an informative empty state.
   */
  it('displays an empty state when no budgets exist', () => {
    budgetService.getBudgets.mockReturnValue(of({ month: '2026-11', budgets: [] }));

    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    expect(html.textContent).toContain("You haven't created any budgets yet.");
  });

  /**
   * Verify that clicking Create Budget actually opens
   * the form instead of testing the component method alone.
   */
  it('opens the create form through the interface', () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;
    const createButton = html.querySelector<HTMLButtonElement>('.budget-manager > button');

    expect(createButton).not.toBeNull();

    createButton!.click();
    fixture.detectChanges();

    expect(component.showForm()).toBe(true);
    expect(html.querySelector('#budget-name')).not.toBeNull();
    expect(html.querySelector('#budget-category')).not.toBeNull();
    expect(html.querySelector('#budget-limit')).not.toBeNull();
  });

  /**
   * Verify that editing displays the selected budget's
   * existing values and locks its category.
   */

  it('opens the edit form with the existing budget values', async () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;
    const editButton = html.querySelector<HTMLButtonElement>('.budget-actions button');

    expect(editButton).not.toBeNull();

    // Click the Edit button in the actual dashboard component.
    editButton!.click();

    // Allow Angular's form controls to finish initializing
    // and synchronizing their values through ngModel.
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();

    const nameInput = html.querySelector<HTMLInputElement>('#budget-name');
    const categorySelect = html.querySelector<HTMLSelectElement>('#budget-category');

    // Verify both the component state and displayed form values.
    expect(component.editingId()).toBe(1);
    expect(component.formName).toBe('Groceries');
    expect(nameInput?.value).toBe('Groceries');
    expect(categorySelect?.disabled).toBe(true);
    expect(html.textContent).toContain('Save Changes');
  });

  /**
   * A loading message should appear while the server
   * has not yet responded.
   */
  it('renders the loading state', () => {
    const response = new Subject<{
      month: string;
      budgets: Budget[];
    }>();

    budgetService.getBudgets.mockReturnValue(response.asObservable());

    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    expect(html.textContent).toContain('Loading budgets...');

    response.next({
      month: '2026-11',
      budgets: [groceryBudget],
    });
    response.complete();

    fixture.detectChanges();

    expect(html.textContent).toContain('Groceries');
    expect(html.textContent).not.toContain('Loading budgets...');
  });

  /**
   * Over-budget and inactive states should both be
   * understandable to users.
   */
  it('renders over-budget and inactive indicators', () => {
    fixture.detectChanges();

    component.budgets.set([
      {
        ...groceryBudget,
        id: 2,
        name: 'Shopping',
        category: 'SHOPPING',
        active: false,
      },
      {
        ...groceryBudget,
        id: 3,
        name: 'Entertainment',
        category: 'ENTERTAINMENT',
        spent: 600,
        remaining: 0,
        percent_used: 150,
        over_budget: true,
      },
    ]);

    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    expect(html.textContent).toContain('Inactive');
    expect(html.textContent).toContain('Over budget!');
    expect(html.textContent).toContain('150.0% used');
    expect(html.querySelectorAll('.budget-card').length).toBe(2);
  });

  /**
   * Ensure API failures are visible on the actual page.
   */
  it('renders an error when loading fails', () => {
    budgetService.getBudgets.mockReturnValue(throwError(() => new Error('Network unavailable')));

    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;
    const alert = html.querySelector('[role="alert"]');

    expect(alert?.textContent).toContain('Unable to load budgets');
  });

  /**
   * Saving and successful operations should update the
   * interface and its accessible status messages.
   */
  it('renders saving and success states', () => {
    fixture.detectChanges();

    component.startCreate();
    component.saving.set(true);
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;
    const submitButton = html.querySelector<HTMLButtonElement>('button[type="submit"]');

    expect(submitButton?.disabled).toBe(true);
    expect(submitButton?.textContent).toContain('Saving...');

    component.saving.set(false);
    component.cancelForm();
    component.successMessage.set('Budget updated successfully!');
    fixture.detectChanges();

    expect(html.querySelector('[role="status"]')?.textContent).toContain(
      'Budget updated successfully!',
    );
  });

  it('changes the displayed month using the dropdown', async () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;
    const monthSelect = html.querySelector<HTMLSelectElement>('#budget-month');

    expect(monthSelect).not.toBeNull();

    // Choose the previous month from the generated options.
    const previousMonth = component.monthOptions[1].value;

    monthSelect!.value = previousMonth;
    monthSelect!.dispatchEvent(new Event('change', { bubbles: true }));

    await fixture.whenStable();
    fixture.detectChanges();

    expect(component.selectedMonth).toBe(previousMonth);
    expect(budgetService.getBudgets).toHaveBeenCalledWith(previousMonth);
  });

  it('closes the create form when Cancel is clicked', async () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    component.startCreate();
    fixture.detectChanges();
    await fixture.whenStable();

    const cancelButton = html.querySelector<HTMLButtonElement>(
      '.form-actions button[type="button"]',
    );

    expect(cancelButton).not.toBeNull();

    cancelButton!.click();
    fixture.detectChanges();

    expect(component.showForm()).toBe(false);
    expect(component.formName).toBe('');
    expect(component.editingId()).toBe(null);
  });

  it('submits a new budget using the actual form', async () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    component.startCreate();
    component.formName = 'Entertainment';
    component.formCategory = 'ENTERTAINMENT';
    component.formMonthlyLimit = 200;

    fixture.detectChanges();
    await fixture.whenStable();

    const form = html.querySelector<HTMLFormElement>('.budget-form');

    expect(form).not.toBeNull();

    form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));

    fixture.detectChanges();

    expect(budgetService.createBudget).toHaveBeenCalledWith({
      name: 'Entertainment',
      category: 'ENTERTAINMENT',
      monthly_limit: 200,
      active: true,
    });

    expect(component.showForm()).toBe(false);
    expect(component.successMessage()).toBe('Budget created successfully!');
  });

  it('submits changes to an existing budget using the form', async () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    component.startEdit(groceryBudget);
    component.formMonthlyLimit = 600;

    fixture.detectChanges();
    await fixture.whenStable();

    const form = html.querySelector<HTMLFormElement>('.budget-form');

    expect(form).not.toBeNull();

    form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));

    fixture.detectChanges();

    expect(budgetService.updateBudget).toHaveBeenCalledWith(groceryBudget.id, {
      name: 'Groceries',
      monthly_limit: 600,
      active: true,
    });

    expect(component.successMessage()).toBe('Budget updated successfully!');
  });

  it('deletes a budget when its Delete button is clicked', () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    try {
      fixture.detectChanges();

      const html = fixture.nativeElement as HTMLElement;
      const deleteButton = html.querySelector<HTMLButtonElement>(
        '.budget-actions button:last-child',
      );

      expect(deleteButton).not.toBeNull();

      deleteButton!.click();
      fixture.detectChanges();

      expect(confirmSpy).toHaveBeenCalled();
      expect(budgetService.deleteBudget).toHaveBeenCalledWith(1);
      expect(component.successMessage()).toBe('Budget deleted successfully!');
    } finally {
      confirmSpy.mockRestore();
    }
  });

  it('does not delete a budget when the user cancels from the interface', () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);

    try {
      fixture.detectChanges();

      const html = fixture.nativeElement as HTMLElement;
      const deleteButton = html.querySelector<HTMLButtonElement>(
        '.budget-actions button:last-child',
      );

      expect(deleteButton).not.toBeNull();

      deleteButton!.click();

      expect(budgetService.deleteBudget).not.toHaveBeenCalled();
    } finally {
      confirmSpy.mockRestore();
    }
  });

  it('disables budget actions while a deletion is in progress', () => {
    fixture.detectChanges();

    component.deletingId.set(groceryBudget.id);
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;
    const buttons = html.querySelectorAll<HTMLButtonElement>('.budget-actions button');

    expect(buttons.length).toBe(2);
    expect(buttons[0].disabled).toBe(true);
    expect(buttons[1].disabled).toBe(true);
    expect(buttons[1].textContent).toContain('Deleting...');
  });

  it('shows a validation error when an invalid form is submitted', async () => {
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    component.startCreate();
    component.formName = '';
    component.formMonthlyLimit = 400;

    fixture.detectChanges();
    await fixture.whenStable();

    const form = html.querySelector<HTMLFormElement>('.budget-form');

    expect(form).not.toBeNull();

    form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));

    fixture.detectChanges();

    expect(budgetService.createBudget).not.toHaveBeenCalled();

    const alert = html.querySelector('[role="alert"]');

    expect(alert?.textContent).toContain('Please enter a budget name.');
  });

  it('updates budget form fields through user input', async () => {
    fixture.detectChanges();

    // Open the real create-budget form.
    component.startCreate();
    fixture.detectChanges();
    await fixture.whenStable();

    const html = fixture.nativeElement as HTMLElement;

    const nameInput = html.querySelector<HTMLInputElement>('#budget-name');
    const categorySelect = html.querySelector<HTMLSelectElement>('#budget-category');
    const limitInput = html.querySelector<HTMLInputElement>('#budget-limit');
    const activeCheckbox = html.querySelector<HTMLInputElement>('input[name="budgetActive"]');

    expect(nameInput).not.toBeNull();
    expect(categorySelect).not.toBeNull();
    expect(limitInput).not.toBeNull();
    expect(activeCheckbox).not.toBeNull();

    // Simulate typing a budget name.
    nameInput!.value = 'Restaurants';
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));

    // Simulate selecting a spending category.
    categorySelect!.value = 'FOOD_DINING';
    categorySelect!.dispatchEvent(new Event('change', { bubbles: true }));

    // Simulate typing a monthly spending limit.
    limitInput!.value = '350';
    limitInput!.dispatchEvent(new Event('input', { bubbles: true }));

    // Simulate disabling the budget.
    activeCheckbox!.checked = false;
    activeCheckbox!.dispatchEvent(new Event('change', { bubbles: true }));

    await fixture.whenStable();
    fixture.detectChanges();

    // Verify the browser controls updated the component properties.
    expect(component.formName).toBe('Restaurants');
    expect(component.formCategory).toBe('FOOD_DINING');
    expect(component.formMonthlyLimit).toBe(350);
    expect(component.formActive).toBe(false);
  });

  it('shows a duplicate category error inside the create form', () => {
    budgetService.createBudget.mockReturnValue(
      throwError(() => new HttpErrorResponse({ status: 409 })),
    );

    fixture.detectChanges();

    component.startCreate();
    component.formName = 'Another Grocery Budget';
    component.formCategory = 'GROCERIES';
    component.formMonthlyLimit = 300;

    component.saveBudget();
    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;
    const form = html.querySelector('.budget-form');
    const alert = form?.querySelector('[role="alert"]');

    expect(alert?.textContent).toContain('You already have a budget for this category.');
  });

  it('displays readable budget category names', () => {
    expect(component.categoryLabel('FOOD_DINING')).toBe('Food & Dining');
    expect(component.categoryLabel('GROCERIES')).toBe('Groceries');
    expect(component.categoryLabel('OTHER')).toBe('Other');

    fixture.detectChanges();

    const html = fixture.nativeElement as HTMLElement;

    expect(html.querySelector('.budget-card')?.textContent).toContain('Groceries');
  });
});
