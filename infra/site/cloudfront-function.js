// CloudFront Function (viewer-request) for nofriction.io.
// Rewrites clean URLs to the S3 object behind them:
//   /support        -> /support/index.html   (after a 301 to /support/)
//   /support/       -> /support/index.html
//   /privacy.html   -> served as-is (the redirect stub)
// Missing objects are handled by the distribution's custom error response (404 -> /404.html).
function handler(event) {
  var request = event.request;
  var uri = request.uri;
  var host = request.headers.host && request.headers.host.value;

  // www -> apex
  if (host === 'www.nofriction.io') {
    return {
      statusCode: 301,
      statusDescription: 'Moved Permanently',
      headers: { location: { value: 'https://nofriction.io' + uri + (request.querystring && Object.keys(request.querystring).length ? '?' + buildQuery(request.querystring) : '') } }
    };
  }

  // Legacy flat URLs from the first draft
  if (uri === '/support.html') return redirect('/support/');
  if (uri === '/terms.html') return redirect('/terms/');

  // /path -> /path/ (directory-style canonical) when there is no file extension
  if (uri.length > 1 && !uri.endsWith('/') && uri.lastIndexOf('.') < uri.lastIndexOf('/')) {
    return redirect(uri + '/');
  }
  // /path/ -> /path/index.html
  if (uri.endsWith('/')) {
    request.uri = uri + 'index.html';
  }
  return request;
}

function redirect(location) {
  return { statusCode: 301, statusDescription: 'Moved Permanently', headers: { location: { value: location } } };
}

function buildQuery(qs) {
  var parts = [];
  for (var k in qs) {
    if (qs[k].multiValue) {
      qs[k].multiValue.forEach(function (v) { parts.push(k + '=' + v.value); });
    } else {
      parts.push(k + '=' + qs[k].value);
    }
  }
  return parts.join('&');
}
