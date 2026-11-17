The scrolling fixture is synthetic local HTML with numbered, repeated sections.
`scroll-repeated-sections.png` is a 1698 × 1080 native capture of sections 5–7 and
the preceding section tail, including a cursor highlight over section 6.
The scrolling regression removes that small highlight and verifies that the
otherwise stationary page does not gain a duplicate section. No user content
or browser chrome is included. Open the HTML in a browser for a native scroll
check; each section should appear once, in order, in the completed image.
