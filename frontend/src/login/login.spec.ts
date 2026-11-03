import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';

import { Login } from './login';
import { AuthService } from '../app/services/auth.service';

describe('Login', () => {
  let component: Login;
  let authService: {
    login: ReturnType<typeof vi.fn>;
  };
  let router: {
    navigate: ReturnType<typeof vi.fn>;
  };

  beforeEach(async () => {
    authService = {
      login: vi.fn().mockReturnValue(of(void 0)),
    };

    router = {
      navigate: vi.fn(),
    };

    await TestBed.configureTestingModule({
      imports: [Login],
      providers: [
        { provide: AuthService, useValue: authService },
        { provide: Router, useValue: router },
      ],
    })
      .overrideComponent(Login, {
        set: { template: '' },
      })
      .compileComponents();

    const fixture = TestBed.createComponent(Login);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should reject missing login credentials', () => {
    component.email = '';
    component.password = '';

    component.onLogin();

    expect(component.errorMessage()).toBe('Please enter your email and password.');
    expect(authService.login).not.toHaveBeenCalled();
  });

  it('should submit credentials and navigate after successful login', () => {
    component.email = '  john@example.com  ';
    component.password = 'Password123!';

    component.onLogin();

    expect(authService.login).toHaveBeenCalledWith('john@example.com', 'Password123!');
    expect(router.navigate).toHaveBeenCalledWith(['/dashboard']);
    expect(component.isSubmitting()).toBe(false);
  });

  it('should display an error for invalid credentials', () => {
    authService.login.mockReturnValue(throwError(() => ({ status: 401 })));

    component.email = 'john@example.com';
    component.password = 'WrongPassword';

    component.onLogin();

    expect(component.errorMessage()).toBe('Invalid email or password.');
    expect(component.isSubmitting()).toBe(false);
  });

  it('should display a generic error for server failures', () => {
    authService.login.mockReturnValue(throwError(() => ({ status: 500 })));

    component.email = 'john@example.com';
    component.password = 'Password123!';

    component.onLogin();

    expect(component.errorMessage()).toBe('Unable to log in right now. Please try again.');
  });

  it('should prevent duplicate submissions', () => {
    component.isSubmitting.set(true);
    component.email = 'john@example.com';
    component.password = 'Password123!';

    component.onLogin();

    expect(authService.login).not.toHaveBeenCalled();
  });

  it('should reject a missing password when email is provided', () => {
    component.email = 'john@example.com';
    component.password = '';

    component.onLogin();

    expect(component.errorMessage()).toBe('Please enter your email and password.');
    expect(authService.login).not.toHaveBeenCalled();
  });

  it('should display invalid credentials for a 400 response', () => {
    authService.login.mockReturnValue(throwError(() => ({ status: 400 })));

    component.email = 'john@example.com';
    component.password = 'WrongPassword';

    component.onLogin();

    expect(component.errorMessage()).toBe('Invalid email or password.');
    expect(component.isSubmitting()).toBe(false);
  });
});
