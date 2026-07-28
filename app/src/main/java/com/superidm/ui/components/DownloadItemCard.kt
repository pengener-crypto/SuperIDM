package com.superidm.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.FilePresent
import androidx.compose.material.icons.filled.OpenInNew
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.superidm.engine.DownloadEngine
import com.superidm.engine.DownloadProgress
import com.superidm.data.db.DownloadEntity
import com.superidm.data.model.DownloadStatus
import com.superidm.ui.theme.BrandGradient
import com.superidm.util.FormatUtils

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DownloadItemCard(
    entity: DownloadEntity,
    progress: DownloadProgress?,
    onPause: () -> Unit,
    onResume: () -> Unit,
    onCancel: () -> Unit,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier
) {
    val dismissState = rememberSwipeToDismissBoxState()

    SwipeToDismissBox(
        state = dismissState,
        backgroundContent = {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.error)
                    .padding(16.dp),
                contentAlignment = Alignment.CenterEnd
            ) {
                Icon(Icons.Default.Cancel, contentDescription = "Cancel", tint = Color.White)
            }
        },
        content = {
            Card(
                modifier = modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                elevation = CardDefaults.cardElevation(defaultElevation = 4.dp),
                shape = RoundedCornerShape(16.dp)
            ) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            imageVector = Icons.Default.FilePresent,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(40.dp)
                        )
                        Spacer(modifier = Modifier.width(16.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = entity.fileName.ifEmpty { entity.url },
                                style = MaterialTheme.typography.titleMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis
                            )
                            Spacer(modifier = Modifier.height(4.dp))
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    text = entity.status.name,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = if (entity.status == DownloadStatus.FAILED) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.secondary
                                )
                                Spacer(modifier = Modifier.width(8.dp))
                                if (progress != null && progress.speed > 0) {
                                    SpeedBadge(speed = progress.speed)
                                }
                            }
                        }
                    }

                    Spacer(modifier = Modifier.height(12.dp))

                    val currentProgress = progress?.downloadedBytes ?: entity.downloadedBytes
                    val total = progress?.totalBytes ?: entity.totalBytes
                    val progressFraction = if (total > 0) currentProgress.toFloat() / total.toFloat() else 0f

                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(8.dp)
                            .clip(RoundedCornerShape(4.dp))
                            .background(MaterialTheme.colorScheme.surfaceVariant)
                    ) {
                        Box(
                            modifier = Modifier
                                .fillMaxHeight()
                                .fillMaxWidth(progressFraction)
                                .background(BrandGradient)
                        )
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        Text(
                            text = "${FormatUtils.formatSize(currentProgress)} / ${FormatUtils.formatSize(total)}",
                            style = MaterialTheme.typography.bodySmall
                        )
                        if (progress != null && progress.eta > 0) {
                            Text(
                                text = FormatUtils.formatEta(progress.eta),
                                style = MaterialTheme.typography.bodySmall
                            )
                        }
                    }

                    if (entity.status == DownloadStatus.FAILED && entity.errorMessage != null) {
                        Spacer(modifier = Modifier.height(4.dp))
                        Text(
                            text = entity.errorMessage,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error
                        )
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                        if (entity.status == DownloadStatus.DOWNLOADING || entity.status == DownloadStatus.CONNECTING) {
                            IconButton(onClick = onPause) {
                                Icon(Icons.Default.Pause, contentDescription = "Pause")
                            }
                        } else if (entity.status == DownloadStatus.PAUSED || entity.status == DownloadStatus.FAILED) {
                            IconButton(onClick = onResume) {
                                Icon(Icons.Default.PlayArrow, contentDescription = "Resume")
                            }
                        }
                        
                        if (entity.status != DownloadStatus.COMPLETED) {
                            IconButton(onClick = onCancel) {
                                Icon(Icons.Default.Cancel, contentDescription = "Cancel")
                            }
                        } else {
                            IconButton(onClick = onOpen) {
                                Icon(Icons.Default.OpenInNew, contentDescription = "Open")
                            }
                        }
                    }
                }
            }
        }
    )
}
