package com.superidm.ui.screens

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.os.Environment
import android.webkit.*
import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.*
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.browser.AdBlocker
import com.superidm.browser.BrowserViewModel
import com.superidm.browser.VideoDetector

@SuppressLint("SetJavaScriptEnabled", "JavascriptInterface")
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BrowserScreen(
    initialUrl: String, 
    isPrivateMode: Boolean = false, 
    viewModel: BrowserViewModel = hiltViewModel(), 
    onNavigateBack: () -> Unit
) {
    val context = LocalContext.current
    val url by viewModel.url.collectAsState()
    val progress by viewModel.progress.collectAsState()
    val isLoading by viewModel.isLoading.collectAsState()
    val canGoBack by viewModel.canGoBack.collectAsState()
    val canGoForward by viewModel.canGoForward.collectAsState()
    val detectedVideos by viewModel.detectedVideos.collectAsState()
    val pendingDownload by viewModel.pendingDownload.collectAsState()
    val tabs by viewModel.tabs.collectAsState()
    
    val webViewRef = remember { mutableStateOf<WebView?>(null) }
    var urlInput by remember { mutableStateOf(initialUrl) }
    var isUrlFocused by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        viewModel.loadUrl(initialUrl)
    }

    LaunchedEffect(url) {
        if (!isUrlFocused) {
            urlInput = url
        }
    }

    val backgroundColor = if (isPrivateMode) Color(0xFF1E112A) else MaterialTheme.colorScheme.background

    Scaffold(
        topBar = {
            Column {
                TopAppBar(
                    title = {
                        OutlinedTextField(
                            value = urlInput,
                            onValueChange = { urlInput = it },
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(50.dp)
                                .onFocusChanged { focusState ->
                                    isUrlFocused = focusState.isFocused
                                    if (!isUrlFocused) {
                                        urlInput = url
                                    }
                                },
                            singleLine = true,
                            shape = RoundedCornerShape(24.dp),
                            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
                            keyboardActions = KeyboardActions(
                                onGo = {
                                    viewModel.loadUrl(urlInput)
                                    webViewRef.value?.loadUrl(viewModel.normalizeUrl(urlInput))
                                }
                            ),
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedBorderColor = Color.Transparent,
                                unfocusedBorderColor = Color.Transparent,
                                focusedContainerColor = MaterialTheme.colorScheme.surfaceVariant,
                                unfocusedContainerColor = MaterialTheme.colorScheme.surfaceVariant
                            )
                        )
                    },
                    navigationIcon = {
                        IconButton(onClick = { 
                            if (canGoBack && webViewRef.value?.canGoBack() == true) {
                                webViewRef.value?.goBack()
                            } else {
                                onNavigateBack()
                            }
                        }) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    },
                    actions = {
                        IconButton(onClick = { webViewRef.value?.reload() }) {
                            Icon(Icons.Default.Refresh, contentDescription = "Refresh")
                        }
                        IconButton(onClick = { /* Menu */ }) {
                            Icon(Icons.Default.MoreVert, contentDescription = "Menu")
                        }
                    },
                    colors = TopAppBarDefaults.topAppBarColors(
                        containerColor = backgroundColor
                    )
                )
                
                AnimatedVisibility(visible = isLoading) {
                    LinearProgressIndicator(
                        progress = { progress / 100f },
                        modifier = Modifier.fillMaxWidth()
                    )
                }

                if (tabs.size > 1) {
                    LazyRow(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(backgroundColor)
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        itemsIndexed(tabs) { index, tab ->
                            FilterChip(
                                selected = viewModel.activeTabIndex.collectAsState().value == index,
                                onClick = { viewModel.switchTab(index) },
                                label = { Text(tab.title.take(15)) },
                                trailingIcon = {
                                    IconButton(
                                        onClick = { viewModel.closeTab(tab.id) },
                                        modifier = Modifier.size(16.dp)
                                    ) {
                                        Icon(Icons.Default.Close, contentDescription = "Close tab")
                                    }
                                }
                            )
                        }
                    }
                }
            }
        }
    ) { paddingValues ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
                .background(backgroundColor)
        ) {
            val adBlocker = AdBlocker()
            
            AndroidView(
                factory = { ctx ->
                    WebView(ctx).apply {
                        webViewRef.value = this
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = true
                        
                        addJavascriptInterface(object {
                            @JavascriptInterface
                            fun onVideoDetected(videoUrl: String, type: String) {
                                viewModel.onVideoDetected(videoUrl, type)
                            }
                        }, VideoDetector.JS_INTERFACE_NAME)

                        setDownloadListener { dUrl, userAgent, contentDisposition, mimeType, contentLength ->
                            viewModel.onDownloadIntercepted(dUrl, userAgent, contentDisposition, mimeType, contentLength)
                        }

                        webViewClient = object : WebViewClient() {
                            override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
                                super.onPageStarted(view, url, favicon)
                                url?.let { viewModel.onPageStarted(it) }
                                viewModel.setNavigation(canGoBack(), canGoForward())
                            }

                            override fun onPageFinished(view: WebView?, url: String?) {
                                super.onPageFinished(view, url)
                                url?.let { viewModel.onPageFinished(it, view?.title ?: "") }
                                viewModel.setNavigation(canGoBack(), canGoForward())
                                view?.evaluateJavascript(viewModel.videoDetector.JS_INJECTION_SCRIPT, null)
                            }

                            override fun shouldInterceptRequest(view: WebView?, request: WebResourceRequest?): WebResourceResponse? {
                                val requestUrl = request?.url?.toString() ?: return super.shouldInterceptRequest(view, request)
                                if (adBlocker.shouldBlock(requestUrl)) {
                                    return adBlocker.getEmptyResponse()
                                }
                                return super.shouldInterceptRequest(view, request)
                            }
                        }

                        webChromeClient = object : WebChromeClient() {
                            override fun onProgressChanged(view: WebView?, newProgress: Int) {
                                super.onProgressChanged(view, newProgress)
                                viewModel.onProgressChanged(newProgress)
                            }
                            
                            override fun onReceivedTitle(view: WebView?, title: String?) {
                                super.onReceivedTitle(view, title)
                                if (url != null && title != null) {
                                    viewModel.onPageFinished(url, title)
                                }
                            }
                        }
                        
                        loadUrl(initialUrl)
                    }
                },
                modifier = Modifier.fillMaxSize(),
                update = { webView ->
                    if (webView.url != url && !isLoading) {
                        webView.loadUrl(url)
                    }
                }
            )

            // Video Detection Banner
            AnimatedVisibility(
                visible = detectedVideos.isNotEmpty(),
                enter = slideInVertically(initialOffsetY = { it }),
                exit = slideOutVertically(targetOffsetY = { it }),
                modifier = Modifier.align(Alignment.BottomCenter)
            ) {
                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    elevation = CardDefaults.cardElevation(defaultElevation = 8.dp)
                ) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Icon(Icons.Default.PlayArrow, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Detected ${detectedVideos.size} video(s)", style = MaterialTheme.typography.titleMedium)
                            Spacer(modifier = Modifier.weight(1f))
                            IconButton(onClick = { viewModel.dismissVideoDetection() }) {
                                Icon(Icons.Default.Close, contentDescription = "Dismiss")
                            }
                        }
                        
                        LazyRow(
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            modifier = Modifier.padding(top = 8.dp)
                        ) {
                            itemsIndexed(detectedVideos) { _, video ->
                                Button(onClick = {
                                    // Trigger download flow
                                }) {
                                    Text("Download ${video.type.uppercase()}")
                                }
                            }
                        }
                    }
                }
            }

            // Download Confirmation Dialog
            if (pendingDownload != null) {
                val download = pendingDownload!!
                AlertDialog(
                    onDismissRequest = { viewModel.dismissDownload() },
                    title = { Text("Download File") },
                    text = {
                        Column {
                            Text("Name: ${download.suggestedFileName}")
                            Text("Size: ${if (download.contentLength > 0) "${download.contentLength / 1024} KB" else "Unknown"}")
                            Text("Type: ${download.mimeType}")
                        }
                    },
                    confirmButton = {
                        Button(onClick = { 
                            val dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath
                            viewModel.confirmDownload(download, dir) 
                        }) {
                            Text("Download")
                        }
                    },
                    dismissButton = {
                        TextButton(onClick = { viewModel.dismissDownload() }) {
                            Text("Cancel")
                        }
                    }
                )
            }
        }
    }
}
