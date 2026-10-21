import { Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';

@Component({
  imports: [],
  selector: 'app-root',
  styleUrl: './login.css',
  templateUrl: './login.html',
})
export class Login {
  protected readonly title = signal('login');
  router = inject(Router);

  onLogin(): void {
    this.router.navigate(['/dashboard']);
  }
}
