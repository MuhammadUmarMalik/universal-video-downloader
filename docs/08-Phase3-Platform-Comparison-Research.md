# Phase 3 Platform Comparison Research

## YouTube — official documentation findings

The official [YouTube Data API reference](https://developers.google.com/youtube/v3/docs) describes resources for videos, playlists, channels, captions, and related metadata. It requires an API key or OAuth 2.0 token for requests, and its documented media-byte operations include caption downloads and uploads, not a general public-video media-file download endpoint.[1]

For the first end-to-end adapter, this means the official Data API is strong for metadata and collection discovery but does not by itself provide the media-byte retrieval path required by the project’s `resolve_formats` and download-plan flow. Any implementation that retrieves public video bytes through undocumented or access-control-sensitive mechanisms would require a separate compliance and security decision and must not bypass controls.

[1]: https://developers.google.com/youtube/v3/docs "YouTube Data API Reference"

## TikTok — official documentation findings

TikTok’s official Display API is designed to display a creator’s profile and videos. Its documented endpoints provide user information and video metadata, with the `video.list` permission described as reading a user’s public videos. The examples emphasize embedding or viewing videos in a webview, and the page does not document a general media-byte download endpoint.[2]

The Display API therefore offers useful metadata only for an authorized Display API integration; it also requires the user/account permission model described by TikTok. It is not a clean fit for a local-first public-URL downloader that must avoid credential or session handling.

[2]: https://developers.tiktok.com/docs/en/display-api-overview?enter_method=left_navigation "TikTok Display API Overview"

## Vimeo — official documentation findings

Vimeo documents direct video-file links through its API, but the feature requires an eligible paid plan and an authentication token with `public`, `private`, and `video_files` scopes. The returned download links may expire, and the documentation frames the capability around videos in the authenticated account.[3]

Vimeo has the clearest documented media-byte path among the candidates, but it conflicts with the project’s no-credential/no-cookie Phase 3 boundary for arbitrary public URLs. A compliant adapter could target explicitly authorized, user-owned Vimeo content only if the product scope and credential policy are changed later; it is not suitable for the current first adapter.

[3]: https://help.vimeo.com/hc/en-us/articles/12427806914577-About-video-file-download-links-from-the-API "Vimeo: About video file download links from the API"

## Instagram — official documentation findings

Meta’s official Instagram API overview describes the Graph API as an API for businesses and creators managing professional Instagram accounts. It also describes Basic Display as allowing people to import their own media and connect their own profiles. The documented product scope is account- and permission-oriented rather than an anonymous public-URL media downloader, and the overview does not document a general direct media-byte download endpoint for arbitrary posts.[4]

Instagram is therefore a poor first adapter under the current rules: a compliant implementation would need an authenticated account/permission model or would otherwise risk relying on undocumented access paths. Both conflict with the current no-credential/no-cookie boundary.

[4]: https://developers.facebook.com/products/instagram/apis/ "Instagram APIs | Meta for Developers"

## Reddit — official documentation findings

Reddit’s official API reference is an automatically generated catalog of API methods and listings. The retrieved reference is strong evidence for public listing and metadata-style API operations, but it does not provide a clear stable contract for direct hosted-video byte retrieval in the documented API surface.[5]

Reddit may be a viable metadata/discovery adapter for public submissions, but its media-download path is less explicit and may depend on the media host or a separate authorized resource. It should not be selected first unless the adapter can be limited to clearly documented public media URLs and tested without anti-bot, credential, cookie, or private-content behavior.

[5]: https://www.reddit.com/dev/api/ "Reddit API documentation"

## Facebook — official documentation findings

Meta’s Page Public Content Access feature allows an app to read public Page posts, comments, and business metadata for analysis or display. Meta states that live access requires App Review and business verification; testing access is limited to Pages whose administrators are also app roles, and live apps cannot see other Page public content without the feature.[6]

The official feature is therefore designed for reviewed Page-content analytics/display, not anonymous public video-byte downloading. It introduces a substantial account, review, and business-verification burden and does not provide a clean no-credential media-download contract for the first adapter.

[6]: https://developers.facebook.com/docs/features-reference/page-public-content-access/ "Page Public Content Access | Meta for Developers"

## X/Twitter — official documentation findings

X’s official documentation describes the X API as programmatic access to the public conversation, with field-level Post/User/Media data and separate media-upload endpoints. The documented upload pages concern sending media to X and attaching it to posts; the reviewed official pages did not expose a general public-post video-byte download endpoint suitable for the adapter contract.[7]

X is therefore metadata-oriented for this decision and has an unclear compliant media-byte path. It should not be the first adapter under the current no-credential/no-bypass scope without a more specific, officially supported public-media retrieval contract.

[7]: https://docs.x.com/x-api/introduction "X API introduction"

## 7. Comparative decision matrix

The scores below are relative engineering-fit scores for the current project constraints, not claims about platform policy approval. A higher score means a better fit for a first adapter that must handle public URLs without credentials, cookies, browser sessions, or access-control bypasses.

| Platform | Public metadata fit | Documented media-byte fit | No-credential fit | Adapter risk | First-adapter fit |
|---|---:|---:|---:|---|---:|
| YouTube | High | Low through official Data API | Low to medium | High policy and implementation risk | Low |
| TikTok | Medium for authorized Display API use | Low | Low | High account/permission dependency | Low |
| Instagram | Low to medium for account-oriented APIs | Low | Low | High account/permission dependency | Low |
| Facebook | Medium for reviewed Page public content | Low | Low | High App Review/business-verification dependency | Low |
| Vimeo | High for authorized account content | High for eligible authenticated account content | Very low | Credential, plan, and expiring-link dependency | Low under current rules |
| Reddit | High for public post discovery and metadata | Medium, conditional on a clearly public media URL | High for public-resource-only scope | Medium; official API byte-download contract is less explicit | **Highest conditional fit** |
| X/Twitter | Medium for public post/media metadata | Low or unclear | Low to medium | API access and media-delivery ambiguity | Low |

## 8. Recommendation

**Recommended first end-to-end adapter: Reddit.** Reddit is the strongest fit for the current Phase 3 constraints because the first slice can be restricted to public post URLs and publicly retrievable media resources, without adding OAuth, cookies, browser automation, private-content access, or anti-bot behavior. The adapter can exercise the full architecture—URL detection, normalization, metadata analysis, format resolution, snapshot persistence, job creation, and download-plan handoff—while keeping the supported scope deliberately narrow.

This recommendation is conditional on a fail-closed implementation spike: the adapter must proceed only when the public post response exposes a directly retrievable media resource that is valid for the user’s authorized public-media use. If the resource is absent, requires a session, requires a CAPTCHA, is private/restricted, or otherwise depends on bypass behavior, the adapter must return a typed unsupported/access-restricted error rather than attempting extraction or circumvention.

The first Reddit slice should support one public Reddit post containing one Reddit-hosted video, with metadata and publicly available video representations only. It should not initially support authenticated communities, private/quarantined content, browser-session reuse, third-party embedded hosts, gallery expansion, livestream capture, or any access-control workaround. Audio/video merging may remain a later media-processing concern; the Phase 3 adapter should report only formats it can identify through an explicit public resource contract.

## 9. Alternatives and trade-offs

**Vimeo** is the strongest technical media-download candidate only if the product later permits authenticated, user-authorized account content. Vimeo explicitly documents direct file links, but it requires an eligible paid plan and an authentication token with file-related scopes, which conflicts with the current no-credential/no-cookie rule.[3]

**YouTube** has the broadest product value and excellent metadata/playlist support, but the official Data API reference does not provide a general public-video media-byte download operation. It should not be the first adapter unless the project separately approves a compliant media-delivery strategy that does not bypass access controls.[1]

**TikTok, Instagram, and Facebook** expose useful account- or review-oriented APIs, but their official documentation is centered on authenticated creators, professional accounts, Page access, display, analytics, or publishing rather than anonymous public media-byte retrieval.[2] [4] [6]

**X/Twitter** provides public-conversation and media metadata APIs, but the reviewed official pages did not establish a stable public-post video-byte download contract. It is better treated as a later adapter after a documented media-delivery path is confirmed.[7]

## 10. Decision checkpoint

The project should proceed with **Reddit as the first adapter** only after the user confirms this recommendation. No Phase 3 code has been written as part of this review.

If Reddit is approved, the next implementation plan should be:

1. Define `RedditAdapter` capabilities and typed URL patterns.
2. Implement URL detection and canonical post normalization for public Reddit post URLs.
3. Add a public-resource-only analysis client with bounded requests and typed failure states.
4. Resolve only explicitly exposed public media representations; fail closed otherwise.
5. Persist the analysis snapshot through the existing `AppServices` transaction coordinator.
6. Add adapter contract tests, fixture tests, malformed-response tests, and security-gate tests.
7. Stop before adding any second platform adapter until the first end-to-end path is reviewed.

## References

[1]: https://developers.google.com/youtube/v3/docs "YouTube Data API Reference"

[2]: https://developers.tiktok.com/docs/en/display-api-overview?enter_method=left_navigation "TikTok Display API Overview"

[3]: https://help.vimeo.com/hc/en-us/articles/12427806914577-About-video-file-download-links-from-the-API "Vimeo: About video file download links from the API"

[4]: https://developers.facebook.com/products/instagram/apis/ "Instagram APIs | Meta for Developers"

[5]: https://www.reddit.com/dev/api/ "Reddit API documentation"

[6]: https://developers.facebook.com/docs/features-reference/page-public-content-access/ "Page Public Content Access | Meta for Developers"

[7]: https://docs.x.com/x-api/introduction "X API introduction"
