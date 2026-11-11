import { Component, inject } from '@angular/core';
import { Router } from '@angular/router';

@Component({
  imports: [],
  selector: 'app-page-not-found',
  styleUrl: './page-not-found.css',
  templateUrl: './page-not-found.html',
})
export class PageNotFound {
  router = inject(Router);

  /** Navigates to the Login page. */
  openLanding(): void {
    this.router.navigate(['/landing']);
  }
}
