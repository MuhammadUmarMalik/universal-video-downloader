# Platform Download Capability Findings — August 2026

## TikTok

TikTok’s official Display API documentation describes `/v2/user/info/`, `/v2/video/list/`, and `/v2/video/query/` for profile and video metadata. Its documented use cases focus on displaying or embedding a creator’s videos, and the `video.list` permission is tied to reading a user’s public videos. The documentation does not provide a general public video-file download endpoint.

Source: https://developers.tiktok.com/docs/en/display-api-overview/

## Instagram

Meta’s official Instagram media documentation requires user access tokens and Instagram permissions for the documented media API. It describes media publishing and media metadata/media URLs for eligible account content, with restrictions including privacy and copyright/licensed-audio cases. The documentation explicitly does not provide a general public video download contract for arbitrary Instagram post URLs.

Source: https://developers.facebook.com/documentation/instagram-platform/instagram-graph-api/reference/ig-user/media

## Engineering decision

Under the project security contract, the app must not add undocumented page extraction, cookie/session handling, authentication bypass, private-content access, CAPTCHA or anti-bot bypass, DRM circumvention, or rate-limit evasion. Therefore, arbitrary Instagram/TikTok social-page URL downloading is not approved. Safe options are detection-only adapters or direct public media URLs supplied by the user and validated by the existing downloader boundary.

## Approved implementation scope

The application now supports a `direct` adapter for user-supplied HTTPS URLs ending in approved media extensions. This path is independent of social-page detection and does not scrape, resolve, or transform Instagram/TikTok URLs. Instagram and TikTok remain detection-only adapters that return a typed unavailable result when analysis would require an unsupported public media-byte path.
